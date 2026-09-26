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

    /// Only one member may commit per epoch. Two members admitting the same
    /// joiner produce competing commits and fork the group, so for now the
    /// member who created the group is the only one who admits anyone.
    ///
    /// The real fix is for the relay to serialise commits the way an MLS
    /// delivery service does; until then this keeps the invariant obvious.
    private var isHost = false

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

    func connect(to url: URL, hosting: Bool) {
        status = .connecting
        let task = URLSession.shared.webSocketTask(with: url)
        socket = task
        task.resume()
        receiveLoop()

        isHost = hosting
        do {
            if hosting {
                // Host opens the group and waits for others to announce.
                try client.createGroup()
                status = .joined(members: Int(client.memberCount()))
                note("created the group")
                sendAutoIfReady()
            } else {
                // Everyone else offers a KeyPackage and waits for a Welcome.
                try send(kind: .keyPackage, body: client.keyPackage())
                status = .waitingForGroup
                note("announced key package")
            }
        } catch {
            fail(error)
        }
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
            // The relay never echoes a sender its own frame, so show it locally.
            print("[\(identity)] sent: \(text)")
            lines.append(Line(author: identity, text: text, mine: true))
        } catch {
            fail(error)
        }
    }

    private func send(kind: FrameKind, body: Data) throws {
        let frame = encodeFrame(kind: kind, body: body)
        socket?.send(.data(frame)) { [weak self] error in
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
            let frame = try decodeFrame(wire: data)
            switch frame.kind {
            case .keyPackage:
                // Only the host admits, and only once it has a group.
                guard isHost, case .joined = status else { return }
                let invite = try client.addMember(keyPackage: frame.body)
                try send(kind: .welcome, body: invite.welcome)
                // Admitting someone advances the epoch. Members who were
                // already here must apply the commit or they fall out of
                // sync and can no longer decrypt. The new member ignores it,
                // having arrived at the new epoch via the Welcome.
                try send(kind: .commit, body: invite.commit)
                status = .joined(members: Int(client.memberCount()))
                note("admitted a new member")

            case .welcome:
                guard case .waitingForGroup = status else { return }
                try client.join(welcome: frame.body)
                status = .joined(members: Int(client.memberCount()))
                note("joined the group")
                sendAutoIfReady()

            case .commit:
                _ = try client.receive(wire: frame.body)
                status = .joined(members: Int(client.memberCount()))

            case .application:
                if let text = try client.receive(wire: frame.body) {
                    print("[\(identity)] received: \(text)")
                    lines.append(Line(author: "them", text: text, mine: false))
                }
            }
        } catch {
            fail(error)
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
