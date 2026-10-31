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

    init(identity: String) throws {
        self.identity = identity
        let vault = Self.vaultPath(for: identity)
        self.client = try Client.open(path: vault.path, identity: identity)
        // State used to be one encrypted snapshot file. It is not migrated:
        // openmls cannot move group state between stores, and it only ever
        // held development data. Say so, rather than appear to lose it.
        let old = vault.deletingPathExtension().appendingPathExtension("vault")
        if FileManager.default.fileExists(atPath: old.path) {
            note("ignoring \(old.lastPathComponent) from an older version; it can be deleted")
        }
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
        return base.appendingPathComponent("\(identity).db")
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
    /// strand everyone on the first. Either way, the core's `connect`
    /// requests catch it up on whatever it missed before going live.
    func connect(to url: URL, hosting: Bool, channelText: String? = nil) {
        status = .connecting
        let task = URLSession.shared.webSocketTask(with: url)
        socket = task
        task.resume()
        receiveLoop()

        do {
            if client.channel() != nil {
                try resume()
                try enterJoined()
                note("rejoined the group")
                sendAutoIfReady()
            } else if try client.waitingToJoin() {
                // Asked before quitting and not yet let in. The offer is
                // re-sent, in case nobody was online to answer it before.
                try resume()
                status = .waitingForGroup
                note("still waiting to be admitted; asked again")
            } else if hosting {
                // Host opens the group and waits for others to ask in.
                try client.createGroup()
                try resume()
                try enterJoined()
                note("created the group")
                sendAutoIfReady()
            } else {
                // Everyone else offers a KeyPackage on the channel they were
                // given, and reads their own mailbox for the Welcome. The
                // mailbox is read first, in the same request stream: the
                // relay handles a connection's requests in order, so it is
                // being read before anyone can reply to the offer.
                let wanted = try parseChannelId(text: channelText ?? "")
                try client.offer(channel: wanted)
                try resume()
                self.channelText = formatChannelId(channel: wanted)
                status = .waitingForGroup
                note("asked to join")
            }
        } catch {
            fail(error)
        }
    }

    /// Send the core's catch-up requests: everything missed, then live.
    private func resume() throws {
        for request in try client.connect() {
            write(request)
        }
    }

    /// Show the group this client is in.
    private func enterJoined() throws {
        guard let joined = client.channel() else { throw ChatError.noChannel }
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

    /// Hand whatever the relay sent to the core, which applies it in order,
    /// once, and says what to do about it.
    private func handle(_ data: Data) {
        do {
            for effect in try client.deliver(wire: data) {
                try apply(effect)
            }
        } catch {
            fail(error)
        }
    }

    private func apply(_ effect: ClientEffect) throws {
        switch effect {
        case .send(let request):
            write(request)

        case .message(let text):
            print("[\(identity)] received: \(text)")
            lines.append(Line(author: "them", text: text, mine: false))

        case .joined:
            try enterJoined()
            note("joined the group")
            sendAutoIfReady()

        case .admitted:
            status = .joined(members: Int(client.memberCount()))
            note("admitted a new member")

        case .addSuperseded:
            // Someone else's commit was ordered first, and the joiner was
            // very likely admitted by it. A notice, not something to retry.
            status = .joined(members: Int(client.memberCount()))
            note("another member admitted them first")

        case .advanced:
            status = .joined(members: Int(client.memberCount()))

        case .live:
            // Logged, not shown: tells a headless run catch-up finished.
            print("[\(identity)] caught up")

        case .relayReset:
            note("the relay's history was reset; following it from its start")

        case .unavailable(_, let floor, _):
            note(
                "the relay no longer holds what this client missed (it starts at \(floor)); "
                    + "another member has to add it again")
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
