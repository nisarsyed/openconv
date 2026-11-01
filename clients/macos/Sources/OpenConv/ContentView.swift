import SwiftUI

struct ContentView: View {
    @ObservedObject var model: ChatModel
    @State private var relay = "ws://127.0.0.1:8080/ws"
    /// Pasted in by someone joining; shared by whoever created the group.
    @State private var joinChannel = ""
    @State private var confirmingLeave = false

    var body: some View {
        VStack(spacing: 0) {
            header
            if let channel = model.channelText {
                channelBar(channel)
            }
            if !model.joinRequests.isEmpty {
                Divider()
                requests
            }
            Divider()
            transcript
            Divider()
            composer
        }
        .frame(minWidth: 420, minHeight: 480)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(model.status == .offline ? Color.secondary : Color.green)
                .frame(width: 8, height: 8)
            Text(ChatModel.shown(model.identity)).fontWeight(.semibold)
            Text(model.status.label)
                .foregroundStyle(.secondary)
                .font(.callout)

            Spacer()

            if model.status == .offline {
                TextField("relay", text: $relay)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 190)
                Button("Host") { connect(hosting: true) }
                TextField("channel to join", text: $joinChannel)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))
                    .frame(width: 150)
                Button("Join") { connect(hosting: false) }
                    .disabled(joinChannel.trimmingCharacters(in: .whitespaces).isEmpty)
            } else {
                if case .joined = model.status {
                    membersMenu
                    Button("Leave") { confirmingLeave = true }
                        .disabled(model.leaving)
                        .confirmationDialog(
                            "Leave this group?", isPresented: $confirmingLeave
                        ) {
                            Button("Leave", role: .destructive) { model.leave() }
                        } message: {
                            Text(
                                "You will stop receiving its messages. Rejoining needs an invitation."
                            )
                        }
                }
                Button("Disconnect") { model.disconnect() }
            }
        }
        .padding(10)
    }

    /// Requests to join, each needing a member to let them in (#125). Names
    /// are self-declared until identity exists, so they are marked as such,
    /// and an earlier removal of the same identity or name is shown.
    private var requests: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(model.joinRequests) { request in
                HStack(spacing: 8) {
                    Text(ChatModel.shown(request.name)).fontWeight(.semibold)
                    Text(
                        (request.expired
                            ? "asked to join (request ran out; accepting still works)"
                            : "wants to join")
                            + " · name not verified"
                            + ChatModel.describe(request.removedBefore)
                    )
                    .foregroundStyle(request.removedBefore == nil ? Color.secondary : Color.orange)
                    .font(.caption)
                    Spacer()
                    Button("Decline") { model.decline(request) }
                    Button("Accept") { model.accept(request) }
                }
            }
        }
        .padding(10)
    }

    /// Everyone in the group, with Remove beside each other member. Any
    /// member may remove any other until roles exist (milestone 4).
    private var membersMenu: some View {
        Menu("Members") {
            ForEach(model.members) { member in
                if member.me {
                    Text("\(ChatModel.shown(member.name)) (you)")
                } else {
                    Button("Remove \(ChatModel.shown(member.name))") { model.remove(member) }
                }
            }
        }
        .fixedSize()
    }

    /// The channel id, selectable so it can be copied to whoever joins next.
    private func channelBar(_ channel: String) -> some View {
        HStack(spacing: 6) {
            Text("channel").foregroundStyle(.secondary)
            Text(channel)
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
            Spacer()
        }
        .font(.caption)
        .padding(.horizontal, 10)
        .padding(.bottom, 6)
    }

    private var transcript: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 6) {
                    ForEach(model.lines) { line in
                        row(line).id(line.id)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
            }
            .onChange(of: model.lines.count) {
                if let last = model.lines.last {
                    withAnimation { proxy.scrollTo(last.id, anchor: .bottom) }
                }
            }
        }
    }

    @ViewBuilder
    private func row(_ line: ChatModel.Line) -> some View {
        // "·" is a status note and "!" an error; both render quieter than chat.
        if line.author == "·" || line.author == "!" {
            Text(line.text)
                .font(.caption)
                .foregroundStyle(line.author == "!" ? Color.red : Color.secondary)
        } else {
            HStack(alignment: .top, spacing: 6) {
                Text(line.author)
                    .fontWeight(.semibold)
                    .foregroundStyle(line.mine ? Color.accentColor : Color.primary)
                Text(line.text).textSelection(.enabled)
            }
        }
    }

    private var composer: some View {
        HStack(spacing: 8) {
            TextField("Message", text: $model.draft)
                .textFieldStyle(.roundedBorder)
                .onSubmit(model.sendDraft)
            Button("Send", action: model.sendDraft)
                .disabled(!model.canSend)
        }
        .padding(10)
    }

    private func connect(hosting: Bool) {
        guard let url = URL(string: relay) else { return }
        model.connect(to: url, hosting: hosting, channelText: joinChannel)
    }
}
