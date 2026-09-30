import Foundation
import OpenConvCore

/// Drives one client: MLS group state plus the relay connection.
///
/// Everything the relay sees is ciphertext produced by `OpenConvCore`; this
/// type only decides which frames to send and what to do with ones arriving.
@MainActor
final class ChatModel: ObservableObject {
    enum Status: Equatable {
        case offline
        case connecting
        case waitingForGroup
        case joined(members: Int)

        var label: String {
            switch self {
            case .offline: "offline"
            case .connecting: "connecting…"
            case .waitingForGroup: "waiting to be admitted"
            case .joined(let n): "in group · \(n) member\(n == 1 ? "" : "s")"
            }
        }
    }

    /// Failures of this model's own, as opposed to ones the core throws.
    enum ChatError: Error {
        /// Asked to publish to the group before there was a channel for it.
        case noChannel
    }

    struct Line: Identifiable {
        let id = UUID()
        let author: String
        let text: String
        let mine: Bool
    }

    @Published private(set) var status: Status = .offline
    @Published private(set) var lines: [Line] = []
    @Published var draft: String = ""

    let identity: String
    /// Sent automatically once this client is in the group. Used to drive the
    /// app headlessly; nil for normal interactive use.
    var autoSay: String?

    private let client: Client
    private var socket: URLSessionWebSocketTask?
    private var saidAuto = false

    /// The channel this client publishes to: its group's, or the one it is
    /// asking to join.
    private var channel: Data?

    /// The channel as shareable text, once there is one.
    @Published private(set) var channelText: String?

    /// Highest relay sequence seen, per address. The relay numbers each
    /// address's frames separately, so a gap on one means frames were missed
    /// there and this client's view of that order — and therefore of who won
    /// a commit race — can no longer be trusted.
    private var lastSeq: [Data: UInt64] = [:]

    init(identity: String) throws {
        self.identity = identity
        let vault = Self.vaultPath(for: identity)
        self.client = try Client.open(path: vault.path, identity: identity)
    }

    /// Where this identity's encrypted state lives.
    ///
    /// OPENCONV_DATA_DIR overrides the location so several instances — and
    /// the smoke test — can run without sharing state.
    static func vaultPath(for identity: String) -> URL {
        let base: URL
        if let override = ProcessInfo.processInfo.environment["OPENCONV_DATA_DIR"] {
            base = URL(fileURLWithPath: override)
        } else {
            base = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("OpenConv")
        }
        return base.appendingPathComponent("\(identity).vault")
    }

    /// Fires `autoSay` the first time we reach a joined state.
    private func sendAutoIfReady() {
        guard !saidAuto, let text = autoSay, case .joined = status else { return }
        saidAuto = true
        draft = text
        sendDraft()
    }

    var canSend: Bool {
        if case .joined = status { return !draft.trimmingCharacters(in: .whitespaces).isEmpty }
        return false
    }

    // MARK: - Connection

    /// Connect, then host a new group or ask to join `channelText`'s.
    ///
    /// A client restored with a group already rejoins that group's channel,
    /// whichever was asked for. Hosting again would start a second group and
    /// strand everyone on the first.
    func connect(to url: URL, hosting: Bool, channelText: String? = nil) {
        status = .connecting
        lastSeq = [:]
        let task = URLSession.shared.webSocketTask(with: url)
        socket = task
        task.resume()
        receiveLoop()

        do {
            if let existing = client.channel() {
                try enter(existing)
                note("rejoined the group")
                sendAutoIfReady()
            } else if hosting {
                // Host opens the group and waits for others to ask in.
                try client.createGroup()
                guard let created = client.channel() else { throw ChatError.noChannel }
                try enter(created)
                note("created the group")
                sendAutoIfReady()
            } else {
                // Everyone else offers a KeyPackage on the channel they were
                // given, and waits for a Welcome in their own mailbox. Both
                // are subscribed first, in one request: the relay handles a
                // connection's requests in order, so the subscription is in
                // place before anyone can reply to the offer.
                let wanted = try parseChannelId(text: channelText ?? "")
                let offer = try client.keyPackage()
                let mailbox = try client.mailbox(keyPackage: offer)
                try subscribe([mailbox, wanted])
                channel = wanted
                self.channelText = formatChannelId(channel: wanted)
                try publish(to: wanted, kind: .keyPackage, body: offer)
                status = .waitingForGroup
                note("asked to join")
            }
        } catch {
            fail(error)
        }
    }

    /// Start using a channel this client is a member of.
    private func enter(_ joined: Data) throws {
        try subscribe([joined])
        channel = joined
        let text = formatChannelId(channel: joined)
        channelText = text
        status = .joined(members: Int(client.memberCount()))
        // Logged so the headless smoke test can hand it to the next client.
        print("[\(identity)] channel \(text)")
    }

    func disconnect() {
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        status = .offline
    }

    // MARK: - Sending

    func sendDraft() {
        let text = draft.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return }
        draft = ""
        do {
            try send(kind: .application, body: client.send(text: text))
            // Shown now rather than when the relay echoes it back. If that
            // echo lands behind a commit, the core hands back a resend; the
            // text is unchanged, so this line stays as it is.
            print("[\(identity)] sent: \(text)")
            lines.append(Line(author: identity, text: text, mine: true))
        } catch {
            fail(error)
        }
    }

    /// Publish to this client's channel. Throws rather than dropping the
    /// frame, so a message shown as sent was really handed to the socket.
    private func send(kind: FrameKind, body: Data) throws {
        guard let channel else { throw ChatError.noChannel }
        try publish(to: channel, kind: kind, body: body)
    }

    private func publish(to address: Data, kind: FrameKind, body: Data) throws {
        write(try encodePublish(to: address, kind: kind, body: body))
    }

    private func subscribe(_ addresses: [Data]) throws {
        write(try encodeSubscribe(addresses: addresses))
    }

    private func write(_ request: Data) {
        socket?.send(.data(request)) { [weak self] error in
            guard let error else { return }
            Task { @MainActor in self?.fail(error) }
        }
    }

    // MARK: - Receiving

    private func receiveLoop() {
        socket?.receive { [weak self] result in
            Task { @MainActor in
                guard let self else { return }
                switch result {
                case .failure(let error):
                    self.fail(error)
                case .success(let message):
                    if case .data(let data) = message {
                        self.handle(data)
                    }
                    self.receiveLoop()
                }
            }
        }
    }

    private func handle(_ data: Data) {
        do {
            let frame = try decodeDelivery(wire: data)
            checkOrder(frame.address, frame.seq)

            switch frame.kind {
            case .keyPackage:
                // Any member may admit; the relay's ordering decides which
                // commit takes effect if several land at the same epoch.
                guard case .joined = status else { return }
                let commit = try client.proposeAdd(keyPackage: frame.body)
                try send(kind: .commit, body: commit)

            case .welcome:
                guard case .waitingForGroup = status else { return }
                try client.join(welcome: frame.body)
                guard let joined = client.channel() else { throw ChatError.noChannel }
                try enter(joined)
                note("joined the group")
                sendAutoIfReady()

            case .commit, .application:
                // Traffic for a group this client has not joined yet. It
                // cannot be decrypted and is not addressed to us; waiting for
                // a Welcome is the whole point of this state.
                guard case .joined = status else { return }
                try apply(try client.receive(wire: frame.body))
            }
        } catch {
            fail(error)
        }
    }

    /// Act on what the core made of a frame.
    private func apply(_ event: ClientEvent) throws {
        switch event {
        case .message(let text):
            print("[\(identity)] received: \(text)")
            lines.append(Line(author: "them", text: text, mine: false))

        case .admitted(let welcome, let mailbox):
            // Our commit was ordered first, so the add stands and the new
            // member can be let in. The Welcome goes to their mailbox, not
            // the channel: they are not a member of it yet.
            try publish(to: mailbox, kind: .welcome, body: welcome)
            status = .joined(members: Int(client.memberCount()))
            note("admitted a new member")

        case .addSuperseded:
            // Someone else's commit was ordered first. Theirs has been
            // applied; the joiner was very likely admitted by it, so this is
            // a notice rather than something to retry.
            status = .joined(members: Int(client.memberCount()))
            note("another member admitted them first")

        case .advanced:
            status = .joined(members: Int(client.memberCount()))

        case .echo:
            break

        case .resend(let message):
            // Our message landed behind a commit, so everyone discarded it.
            // This is the same text encrypted for the epoch they are in now.
            try send(kind: .application, body: message)
            print("[\(identity)] resent a message that crossed a commit")

        case .stale:
            // Someone else's message that crossed a commit. They resend it,
            // so the copy that arrives next is the one to show.
            break
        }
    }

    /// The relay numbers each address's frames. A gap means this client
    /// missed traffic there and can no longer trust its view of the order, so
    /// say so loudly rather than letting it surface later as an
    /// undecryptable message.
    private func checkOrder(_ address: Data, _ seq: UInt64) {
        defer { lastSeq[address] = max(lastSeq[address] ?? seq, seq) }
        guard let last = lastSeq[address] else { return }

        if seq > last + 1 {
            note("missed \(seq - last - 1) frame(s) from the relay; state may be stale")
        } else if seq <= last {
            // Never expected: the relay numbers and delivers under one lock.
            // Worth saying out loud rather than computing a negative gap —
            // `seq - last - 1` on UInt64 underflows and traps the process.
            note("frame \(seq) arrived after \(last); relay ordering is broken")
        }
    }

    // MARK: - Feedback

    private func note(_ text: String) {
        print("[\(identity)] \(text)")
        lines.append(Line(author: "·", text: text, mine: false))
    }

    private func fail(_ error: Error) {
        print("[\(identity)] error: \(error)")
        lines.append(Line(author: "!", text: "\(error)", mine: false))
    }
}
