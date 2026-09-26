// swift-tools-version:6.0
import PackageDescription

// The Rust core is linked as a static archive built by `cargo build --release`.
// Run scripts/gen-bindings.sh first; it produces the generated sources and
// the archive this package links against.
let rustLib = "../../target/release"

let package = Package(
    name: "OpenConv",
    platforms: [.macOS(.v14)],
    targets: [
        // C headers emitted by UniFFI. Module name must match the `import`
        // inside the generated Swift.
        .target(name: "openconv_coreFFI"),

        // Generated Swift bindings, plus the link to the Rust archive.
        .target(
            name: "OpenConvCore",
            dependencies: ["openconv_coreFFI"],
            linkerSettings: [
                .unsafeFlags(["-L\(rustLib)", "-lopenconv_core"]),
                .linkedFramework("SystemConfiguration"),
            ]
        ),

        // Runtime bridge verification. A plain executable because XCTest and
        // swift-testing both need a full Xcode install.
        .executableTarget(
            name: "BridgeCheck",
            dependencies: ["OpenConvCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),

        .executableTarget(
            name: "OpenConv",
            dependencies: ["OpenConvCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
