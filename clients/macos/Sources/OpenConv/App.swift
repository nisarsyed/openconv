import AppKit
import SwiftUI

@main
struct OpenConvApp: App {
    @StateObject private var model: ChatModel

    init() {
        // Identity is a launch argument so two instances can run side by side:
        //   swift run OpenConv alice
        let name = CommandLine.arguments.dropFirst().first ?? "me"
        // StateObject takes an autoclosure, so build the model first.
        let model: ChatModel
        do {
            model = try ChatModel(identity: name)
        } catch {
            fatalError("could not create client identity: \(error)")
        }
        _model = StateObject(wrappedValue: model)
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
