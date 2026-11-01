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
        /// Removed from the group, or this client's leave was committed.
        case removed

        var label: String {
            switch self {
            case .offline: "offline"
            case .connecting: "connecting…"
            case .waitingForGroup: "waiting for a member to let you in"
            case .joined(let n): "in group · \(n) member\(n == 1 ? "" : "s")"
            case .removed: "not in the group"
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
    /// Everyone in the group, this client included.
    @Published private(set) var members: [GroupMember] = []
    /// Requests to join waiting for this client, or another member, to answer.
    @Published private(set) var joinRequests: [PendingJoin] = []
    /// Asked to leave, and not yet removed.
    @Published private(set) var leaving = false
    /// Caught up on the group's channel since connecting. Nothing is sent
    /// before then: until it has caught up, this client does not know which
    /// epoch the group is in, or whether it is still a member at all. A
    /// message sent from stale state is normally resent once its echo comes
    /// back behind the commit, but a client that turns out to be stranded
    /// starts the group over and never sees that echo, so the message would
    /// be lost.
    @Published private(set) var caughtUp = false
    @Published var draft: String = ""

    let identity: String
    /// Sent automatically once this client is in the group. Used to drive the
    /// app headlessly; nil for normal interactive use.
    var autoSay: String?
    /// Leave once caught up. For driving the app headlessly.
    var autoLeave = false
    /// Remove the member with this name once caught up. Likewise.
    var autoRemove: String?
    /// Accept the request from this name once it arrives. Likewise.
    var autoAccept: String?
    /// Accept every request as it arrives. For headless runs only (the smoke
    /// test, labs), set by OPENCONV_AUTO_ACCEPT; it undoes what approval is
    /// for, so the app never does this on its own.
    private let acceptEveryone =
        ProcessInfo.processInfo.environment["OPENCONV_AUTO_ACCEPT"] == "1"

    private let client: Client
    private var socket: URLSessionWebSocketTask?
    /// While waiting to be let in, renews the request before it runs out: a
    /// join request is only good for an hour (#123).
    private var offerRefresher: Task<Void, Never>?
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

    /// Fires `autoSay` the first time this client is in the group and caught up.
    private func sendAutoIfReady() {
        guard !saidAuto, caughtUp, let text = autoSay, case .joined = status else { return }
        saidAuto = true
        draft = text
        sendDraft()
    }

    var canSend: Bool {
        if case .joined = status, caughtUp {
            return !draft.trimmingCharacters(in: .whitespaces).isEmpty
        }
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
        caughtUp = false
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
                keepRequestFresh()
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
                keepRequestFresh()
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
        offerRefresher?.cancel()
        channel = joined
        let text = formatChannelId(channel: joined)
        channelText = text
        status = .joined(members: Int(client.memberCount()))
        members = client.members()
        leaving = client.isLeaving()
        // Logged so the headless smoke test can hand it to the next client.
        print("[\(identity)] channel \(text)")
    }

    /// Renew the request to join every few minutes while waiting.
    private func keepRequestFresh() {
        offerRefresher?.cancel()
        offerRefresher = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(300))
                guard let self, case .waitingForGroup = self.status else { return }
                do {
                    for request in try self.client.refreshOffers() {
                        self.write(request)
                    }
                } catch {
                    self.fail(error)
                }
            }
        }
    }

    func disconnect() {
        offerRefresher?.cancel()
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        status = .offline
    }

    // MARK: - Sending

    func sendDraft() {
        // Enter submits whether or not the Send button is enabled.
        guard canSend else { return }
        let text = draft.trimmingCharacters(in: .whitespaces)
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
            refresh()
            note("admitted a new member")

        case .superseded:
            // Someone else's commit was ordered first, and very likely did
            // the same thing. A notice, not something to retry.
            refresh()
            note("another member's change landed first")

        case .advanced:
            refresh()

        case .joinRequested:
            refreshRequests()
            for request in joinRequests where acceptEveryone || request.name == autoAccept {
                accept(request)
            }

        case .removed:
            let wasLeaving = leaving
            status = .removed
            members = []
            leaving = false
            channel = nil
            note(wasLeaving ? "left the group" : "removed from the group")

        case .rejoining:
            status = .waitingForGroup
            caughtUp = false
            keepRequestFresh()
            note("asked the group to add this client back")

        case .live(let address):
            // Logged, not shown: tells a headless run catch-up finished.
            print("[\(identity)] caught up")
            if address == channel {
                caughtUp = true
                sendAutoIfReady()
                runAutoMembership()
            }

        case .relayReset:
            note("the relay's history was reset; following it from its start")

        case .unavailable(_, let floor, _):
            note(
                "the relay no longer holds what this client missed (it starts at \(floor)); "
                    + "another member has to add it again")
        }
    }

    // MARK: - Membership

    /// Ask to leave. Another member commits it; until then this client is
    /// still in the group, and asks again if the group moves on first.
    func leave() {
        do {
            write(try client.leave())
            leaving = true
            note("asked to leave")
        } catch {
            fail(error)
        }
    }

    /// Remove another member.
    func remove(_ member: GroupMember) {
        do {
            write(try client.removeMember(index: member.index))
            // They can still ask to join again; a member has to accept them,
            // and their request is flagged as coming from someone removed.
            note("removing \(member.name)")
        } catch {
            fail(error)
        }
    }

    /// Bring the status, member list and requests in line with the core.
    private func refresh() {
        guard client.channel() != nil else { return }
        status = .joined(members: Int(client.memberCount()))
        members = client.members()
        refreshRequests()
    }

    private func refreshRequests() {
        do {
            let before = Set(joinRequests.map(\.id))
            joinRequests = try client.joinRequests()
            for request in joinRequests where !before.contains(request.id) {
                note("\(request.name) asked to join\(Self.describe(request.removedBefore))")
            }
        } catch {
            fail(error)
        }
    }

    /// Let someone in. This approves the person, so even a request that has
    /// run out lets them in when their app next asks.
    func accept(_ request: PendingJoin) {
        do {
            switch try client.accept(id: request.id) {
            case .sending(let requests):
                for message in requests {
                    write(message)
                }
                note("letting \(request.name) in")
            case .queued:
                note("will let \(request.name) in once caught up")
            case .whenTheyNextAsk:
                note(
                    "\(request.name)'s request has run out; they will be let in when their app next asks"
                )
            }
            refreshRequests()
        } catch {
            fail(error)
        }
    }

    /// Turn someone down. Their later requests are ignored too.
    func decline(_ request: PendingJoin) {
        do {
            try client.decline(id: request.id)
            note("declined \(request.name)")
            refreshRequests()
        } catch {
            fail(error)
        }
    }

    /// What to say about an earlier removal beside a request, if there was one.
    static func describe(_ past: EarlierRemoval?) -> String {
        guard let past else { return "" }
        let when = Date(timeIntervalSince1970: TimeInterval(past.at))
            .formatted(date: .abbreviated, time: .shortened)
        let left = past.by == past.name
        return past.sameIdentity
            ? " (they \(left ? "left" : "were removed by \(past.by)") on \(when))"
            : " (someone else named \(past.name) \(left ? "left" : "was removed by \(past.by)") on \(when))"
    }

    /// The headless `leave` and `remove` launch arguments, once caught up.
    private func runAutoMembership() {
        guard case .joined = status else { return }
        refresh()
        if autoLeave {
            autoLeave = false
            leave()
        }
        if let name = autoRemove, let member = members.first(where: { $0.name == name && !$0.me }) {
            autoRemove = nil
            remove(member)
        }
        // A request saved from before this launch raises no new event.
        for request in joinRequests where acceptEveryone || request.name == autoAccept {
            accept(request)
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

extension GroupMember: Identifiable {
    public var id: UInt32 { index }
}

extension PendingJoin: Identifiable {}
