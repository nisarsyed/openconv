import SwiftUI

struct ContentView: View {
    @ObservedObject var model: ChatModel
    @State private var relay = "ws://127.0.0.1:8080/ws"

    var body: some View {
        VStack(spacing: 0) {
            header
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
            Text(model.identity).fontWeight(.semibold)
            Text(model.status.label)
                .foregroundStyle(.secondary)
                .font(.callout)

            Spacer()

            if model.status == .offline {
                TextField("relay", text: $relay)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 190)
                Button("Host") { connect(hosting: true) }
                Button("Join") { connect(hosting: false) }
            } else {
                Button("Disconnect") { model.disconnect() }
            }
        }
        .padding(10)
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
        model.connect(to: url, hosting: hosting)
    }
}
