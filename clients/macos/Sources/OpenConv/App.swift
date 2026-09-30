import AppKit
import SwiftUI

@main
struct OpenConvApp: App {
    static let defaultRelay = "ws://127.0.0.1:8080/ws"

    @StateObject private var model: ChatModel

    init() {
        // print() to a pipe is block-buffered; unbuffer so a launched app can
        // be observed live rather than only at exit.
        setbuf(stdout, nil)

        // Identity is a launch argument so two instances can run side by side:
        //   swift run OpenConv alice
        // Further arguments connect on launch, which is what makes the app
        // driveable without a human clicking:
        //   OpenConv alice host [message]
        //   OpenConv bob join <channel> [message]
        let args = Array(CommandLine.arguments.dropFirst())
        let name = args.first ?? "me"
        let autoConnect = args.dropFirst().first
        // StateObject takes an autoclosure, so build the model first.
        let model: ChatModel
        do {
            model = try ChatModel(identity: name)
        } catch {
            fatalError("could not create client identity: \(error)")
        }
        _model = StateObject(wrappedValue: model)
        if let autoConnect, let url = URL(string: Self.defaultRelay) {
            let hosting = autoConnect == "host"
            // A joiner names the channel next; then, for either, an optional
            // message to send once in the group.
            let rest = Array(args.dropFirst(2))
            let channel = hosting ? nil : rest.first
            model.autoSay = hosting ? rest.first : rest.dropFirst().first
            // Defer until the run loop is up so the socket has somewhere to live.
            Task { @MainActor in
                model.connect(to: url, hosting: hosting, channelText: channel)
            }
        }
        // Running as a bare SwiftPM executable rather than a bundled .app,
        // so ask AppKit for a normal windowed application.
        NSApplication.shared.setActivationPolicy(.regular)
    }

    var body: some Scene {
        Window("OpenConv", id: "main") {
            ContentView(model: model)
                .onAppear { NSApp.activate(ignoringOtherApps: true) }
        }
    }
}
