// Generates the SwiftUI theme's tokens from design/tokens.json (#130).
//
//   swift scripts/gen-theme.swift design/tokens.json \
//       clients/macos/Sources/OpenConv/Theme/Tokens.swift
//
// `just theme` runs this, and `just theme-check` fails if the checked-in file
// is not what it would write. The output is written already formatted, so it
// passes `swift format lint --strict` without being run through the
// formatter: a formatter that changed between machines would otherwise make
// the drift check fail for nothing.
//
// A Swift script rather than Python, so it runs anywhere the app builds,
// with only the Command Line Tools. JSON is read with a small parser of its
// own because Foundation's forgets the order of keys, and the order is the
// design's: the accents as Settings offers them, the type scale from largest
// down, the avatar pairs by number.

import Foundation

// MARK: - Reading JSON, in order

/// A JSON value, with each object's keys in the order the file has them.
indirect enum JSON {
    case object([(String, JSON)])
    case array([JSON])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    subscript(key: String) -> JSON? {
        guard case .object(let members) = self else { return nil }
        return members.first { $0.0 == key }?.1
    }

    /// An object's members, leaving out `$`-prefixed metadata.
    var members: [(String, JSON)] {
        guard case .object(let members) = self else { return [] }
        return members.filter { !$0.0.hasPrefix("$") }
    }

    var string: String? {
        guard case .string(let s) = self else { return nil }
        return s
    }

    var number: Double? {
        guard case .number(let n) = self else { return nil }
        return n
    }

    /// Whether this is a token: an object with a `$value`.
    var isToken: Bool { self["$value"] != nil }
}

struct ParseError: Error, CustomStringConvertible {
    let description: String
}

/// A recursive-descent reader for RFC 8259 JSON.
struct Parser {
    let bytes: [UInt8]
    var at = 0

    init(_ data: Data) { bytes = [UInt8](data) }

    mutating func document() throws -> JSON {
        let value = try self.value()
        skipSpace()
        guard at == bytes.count else { throw fail("trailing characters") }
        return value
    }

    func fail(_ what: String) -> ParseError {
        ParseError(description: "tokens.json, byte \(at): \(what)")
    }

    mutating func skipSpace() {
        while at < bytes.count, [0x20, 0x09, 0x0A, 0x0D].contains(bytes[at]) { at += 1 }
    }

    mutating func expect(_ literal: String) throws {
        for byte in literal.utf8 {
            guard at < bytes.count, bytes[at] == byte else { throw fail("expected \(literal)") }
            at += 1
        }
    }

    mutating func value() throws -> JSON {
        skipSpace()
        guard at < bytes.count else { throw fail("unexpected end") }
        switch bytes[at] {
        case UInt8(ascii: "{"): return try object()
        case UInt8(ascii: "["): return try array()
        case UInt8(ascii: "\""): return .string(try string())
        case UInt8(ascii: "t"):
            try expect("true")
            return .bool(true)
        case UInt8(ascii: "f"):
            try expect("false")
            return .bool(false)
        case UInt8(ascii: "n"):
            try expect("null")
            return .null
        default: return .number(try number())
        }
    }

    mutating func object() throws -> JSON {
        try expect("{")
        var members: [(String, JSON)] = []
        skipSpace()
        if at < bytes.count, bytes[at] == UInt8(ascii: "}") {
            at += 1
            return .object(members)
        }
        while true {
            skipSpace()
            let key = try string()
            skipSpace()
            try expect(":")
            members.append((key, try value()))
            skipSpace()
            if at < bytes.count, bytes[at] == UInt8(ascii: ",") {
                at += 1
                continue
            }
            try expect("}")
            return .object(members)
        }
    }

    mutating func array() throws -> JSON {
        try expect("[")
        var items: [JSON] = []
        skipSpace()
        if at < bytes.count, bytes[at] == UInt8(ascii: "]") {
            at += 1
            return .array(items)
        }
        while true {
            items.append(try value())
            skipSpace()
            if at < bytes.count, bytes[at] == UInt8(ascii: ",") {
                at += 1
                continue
            }
            try expect("]")
            return .array(items)
        }
    }

    mutating func string() throws -> String {
        try expect("\"")
        var out: [UInt8] = []
        while at < bytes.count {
            let byte = bytes[at]
            at += 1
            switch byte {
            case UInt8(ascii: "\""):
                guard let s = String(bytes: out, encoding: .utf8) else {
                    throw fail("a string is not UTF-8")
                }
                return s
            case UInt8(ascii: "\\"):
                guard at < bytes.count else { throw fail("unfinished escape") }
                let escaped = bytes[at]
                at += 1
                let simple: [UInt8: UInt8] = [
                    UInt8(ascii: "\""): 0x22, UInt8(ascii: "\\"): 0x5C, UInt8(ascii: "/"): 0x2F,
                    UInt8(ascii: "b"): 0x08, UInt8(ascii: "f"): 0x0C, UInt8(ascii: "n"): 0x0A,
                    UInt8(ascii: "r"): 0x0D, UInt8(ascii: "t"): 0x09,
                ]
                if let plain = simple[escaped] {
                    out.append(plain)
                } else if escaped == UInt8(ascii: "u") {
                    // Tokens are ASCII in practice; a surrogate pair is not
                    // worth supporting until one turns up.
                    guard at + 4 <= bytes.count,
                        let code = UInt32(String(decoding: bytes[at..<at + 4], as: UTF8.self), radix: 16),
                        let scalar = Unicode.Scalar(code)
                    else { throw fail("bad \\u escape") }
                    at += 4
                    out.append(contentsOf: Array(String(Character(scalar)).utf8))
                } else {
                    throw fail("bad escape")
                }
            default:
                out.append(byte)
            }
        }
        throw fail("unfinished string")
    }

    mutating func number() throws -> Double {
        let start = at
        while at < bytes.count, "+-0123456789.eE".utf8.contains(bytes[at]) { at += 1 }
        guard let n = Double(String(decoding: bytes[start..<at], as: UTF8.self)) else {
            throw fail("bad number")
        }
        return n
    }
}

// MARK: - Turning tokens into Swift

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("gen-theme: \(message)\n".utf8))
    exit(1)
}

/// `caption-strong` → `captionStrong`, `text-2` → `text2`.
func lowerCamel(_ key: String) -> String {
    let parts = key.split(separator: "-").map(String.init)
    guard let first = parts.first else { fail("an empty key") }
    let rest = parts.dropFirst().map { $0.prefix(1).uppercased() + $0.dropFirst() }
    let name = ([first] + rest).joined()
    guard let lead = name.first, lead.isLetter else { fail("\(key) does not start with a letter") }
    return name
}

/// `text-size` → `TextSize`.
func upperCamel(_ key: String) -> String {
    let name = lowerCamel(key)
    return name.prefix(1).uppercased() + name.dropFirst()
}

/// `"26px"` → `"26"`.
func points(_ value: JSON?, _ what: String) -> String {
    guard let text = value?.string, text.hasSuffix("px"), let n = Double(text.dropLast(2)) else {
        fail("\(what) is not a size in px")
    }
    return n == n.rounded() ? String(Int(n)) : String(n)
}

/// `"#0B7470"` → `"0x0B7470FF"`, `"#0B747024"` → `"0x0B747024"`.
func rgba(_ value: JSON?, _ what: String) -> String {
    guard let text = value?.string, text.hasPrefix("#") else { fail("\(what) is not a colour") }
    let digits = text.dropFirst().uppercased()
    guard digits.count == 6 || digits.count == 8, UInt32(digits, radix: 16) != nil else {
        fail("\(what) is not #RRGGBB or #RRGGBBAA")
    }
    return "0x" + digits + (digits.count == 6 ? "FF" : "")
}

/// A colour token's value.
func colour(_ token: JSON?, _ what: String) -> String {
    guard let token, token["$type"]?.string == "color" else { fail("\(what) is not a colour token") }
    return rgba(token["$value"], what)
}

/// The weight SwiftUI calls a CSS font weight.
func weight(_ value: JSON?, _ what: String) -> String {
    switch value?.number {
    case 400: return ".regular"
    case 500: return ".medium"
    case 600: return ".semibold"
    case 700: return ".bold"
    default: fail("\(what) has a weight with no SwiftUI name")
    }
}

/// Lines of output, indented four spaces per level.
struct Output {
    var lines: [String] = []
    var depth = 0

    mutating func line(_ text: String = "") {
        lines.append(text.isEmpty ? "" : String(repeating: "    ", count: depth) + text)
    }

    /// A `///` comment, wrapped to the 100 columns `swift format` allows.
    mutating func doc(_ text: String?) {
        guard let text, !text.isEmpty else { return }
        let width = 100 - depth * 4 - 4
        var current = ""
        for word in text.split(separator: " ") {
            if !current.isEmpty, current.count + 1 + word.count > width {
                line("/// " + current)
                current = ""
            }
            current += (current.isEmpty ? "" : " ") + word
        }
        line("/// " + current)
    }

    mutating func open(_ text: String) {
        line(text + " {")
        depth += 1
    }

    mutating func close() {
        depth -= 1
        line("}")
    }
}

guard CommandLine.arguments.count == 3 else {
    fail("usage: gen-theme.swift <tokens.json> <Tokens.swift>")
}
let source = CommandLine.arguments[1]
let destination = CommandLine.arguments[2]
guard let data = FileManager.default.contents(atPath: source) else { fail("cannot read \(source)") }
let tokens: JSON
do {
    var parser = Parser(data)
    tokens = try parser.document()
} catch {
    fail("\(error)")
}

func section(_ key: String) -> JSON {
    guard let found = tokens[key] else { fail("tokens.json has no \(key)") }
    return found
}

/// A text style's initialiser, from a typography token, with `size` and
/// `lineHeight` replaced if given.
func textStyle(_ key: String, size: String? = nil, lineHeight: String? = nil) -> String {
    guard let style = section("type")[key], let value = style["$value"] else {
        fail("type.\(key) is missing")
    }
    let family = value["fontFamily"]?.string
    guard family == "{font.text}" || family == "{font.display}" else {
        fail("type.\(key) uses neither the text nor the display face")
    }
    return "TextStyle(size: \(size ?? points(value["fontSize"], "type.\(key)")), "
        + "lineHeight: \(lineHeight ?? points(value["lineHeight"], "type.\(key)")), "
        + "weight: \(weight(value["fontWeight"], "type.\(key)"))"
        + (family == "{font.display}" ? ", display: true)" : ")")
}

var out = Output()

out.line("// Generated by scripts/gen-theme.swift from design/tokens.json. Do not edit:")
out.line("// change the tokens, then run `just theme`.")
out.line()
out.line("import AppKit")
out.line("import SwiftUI")
out.line()
out.doc(
    "The design tokens agreed in #133, from `design/tokens.json`. Views take their type, "
        + "spacing, radii, sizes and colours from here rather than from values of their own, "
        + "so the Appearance settings only have to switch which of these a view reads. "
        + "Sizes are in points.")
out.line("enum Tokens {}")

// The building blocks.
out.line()
out.line("// MARK: - Building blocks")
out.line()
out.open("extension Tokens")
out.doc(
    "A colour in light and dark appearances, as 0xRRGGBBAA in sRGB, and the one dynamic "
        + "`NSColor` that shows whichever the view is drawn in.")
out.open("struct Swatch")
out.doc("The colour in a light appearance.")
out.line("let light: UInt32")
out.doc("The colour in a dark appearance.")
out.line("let dark: UInt32")
out.doc("The colour as AppKit draws it: `light` or `dark`, by the appearance it is drawn in.")
out.line("let nsColor: NSColor")
out.line()
out.doc("A colour that is the same in both appearances.")
out.open("init(_ both: UInt32)")
out.line("self.init(light: both, dark: both)")
out.close()
out.line()
out.open("init(light: UInt32, dark: UInt32)")
out.line("self.light = light")
out.line("self.dark = dark")
out.line("nsColor = NSColor(name: nil) { appearance in")
out.depth += 1
out.line("let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua")
out.line("return Swatch.srgb(isDark ? dark : light)")
out.close()
out.close()
out.line()
out.doc("The colour for SwiftUI, following the appearance as `nsColor` does.")
out.line("var color: Color { Color(nsColor: nsColor) }")
out.line()
out.doc("An sRGB colour from 0xRRGGBBAA.")
out.open("static func srgb(_ rgba: UInt32) -> NSColor")
out.line("let channel = { (shift: UInt32) in CGFloat((rgba >> shift) & 0xFF) / 255 }")
out.line("let (red, green, blue) = (channel(24), channel(16), channel(8))")
out.line("return NSColor(srgbRed: red, green: green, blue: blue, alpha: channel(0))")
out.close()
out.close()
out.line()
out.doc("A text style: its size and line height, its weight, and whether it is a title.")
out.open("struct TextStyle: Equatable")
out.line("let size: CGFloat")
out.line("let lineHeight: CGFloat")
out.line("let weight: Font.Weight")
out.doc("Titles are set in the display face, everything else in the text face.")
out.line("let display: Bool")
out.line()
out.open("init(size: CGFloat, lineHeight: CGFloat, weight: Font.Weight, display: Bool = false)")
out.line("self.size = size")
out.line("self.lineHeight = lineHeight")
out.line("self.weight = weight")
out.line("self.display = display")
out.close()
out.line()
out.doc(
    "The system font at this size and weight. SF Pro picks its text or display cut by size, "
        + "so the system font is both faces.")
out.line("var font: Font { .system(size: size, weight: weight) }")
out.close()
out.close()

// Type.
let typeKeys = section("type").members.map { $0.0 }
out.line()
out.line("// MARK: - Type")
out.line()
out.open("extension Tokens")
out.doc("The type scale, at the default text size.")
out.open("enum Typography")
for key in typeKeys {
    out.line("static let \(lowerCamel(key)) = \(textStyle(key))")
}
out.close()

// The text-size steps: the styles they change, as one scale.
guard let textSizes = section("settings")["text-size"],
    let scaled = textSizes.members.first?.1.members.map({ $0.0 })
else { fail("settings.text-size has no steps") }
out.line()
out.doc(
    "The text styles the Text size setting changes, at one of its steps. "
        + (textSizes["$description"]?.string ?? ""))
out.open("struct TextScale: Equatable")
for key in scaled {
    out.line("let \(lowerCamel(key)): TextStyle")
}
out.line()
out.doc("The default step: the type scale as it is.")
out.line("static let standard = TextScale(")
out.depth += 1
for (i, key) in scaled.enumerated() {
    let comma = i == scaled.count - 1 ? "" : ","
    out.line("\(lowerCamel(key)): Typography.\(lowerCamel(key))\(comma)")
}
out.depth -= 1
out.line(")")
for (step, sizes) in textSizes.members {
    guard sizes.members.map({ $0.0 }) == scaled else { fail("text-size.\(step) changes other styles") }
    out.line()
    out.line("static let \(lowerCamel(step)) = TextScale(")
    out.depth += 1
    for (i, (key, size)) in sizes.members.enumerated() {
        let style = textStyle(
            key,
            size: points(size["fontSize"], "text-size.\(step).\(key)"),
            lineHeight: points(size["lineHeight"], "text-size.\(step).\(key)"))
        let comma = i == sizes.members.count - 1 ? "" : ","
        out.line("\(lowerCamel(key)): \(style)\(comma)")
    }
    out.depth -= 1
    out.line(")")
}
out.close()
out.close()

// Space, radii and sizes.
func dimensions(_ title: String, _ key: String, name: (String) -> String, doc: String) {
    out.line()
    out.doc(doc)
    out.open("enum \(title)")
    for (token, value) in section(key).members {
        guard value["$type"]?.string == "dimension" else { fail("\(key).\(token) is not a size") }
        out.doc(value["$description"]?.string)
        out.line("static let \(name(token)): CGFloat = \(points(value["$value"], "\(key).\(token)"))")
    }
    out.close()
}

out.line()
out.line("// MARK: - Space, radii and sizes")
out.line()
out.open("extension Tokens")
dimensions("Space", "space", name: { "s" + $0 }, doc: "Spacing, on a 4pt grid.")
dimensions("Radius", "radius", name: lowerCamel, doc: "Corner radii. Fixed: not a setting.")
dimensions("Size", "size", name: lowerCamel, doc: "Bar, panel and avatar sizes, at comfortable density.")
out.close()

// Colour.
let colours = section("color")
guard let light = colours["light"], let dark = colours["dark"] else {
    fail("color needs light and dark")
}
guard light.members.map({ $0.0 }) == dark.members.map({ $0.0 }) else {
    fail("color.light and color.dark name different colours")
}
out.line()
out.line("// MARK: - Colour")
out.line()
out.open("extension Tokens")
out.doc("The colours, each following the appearance.")
out.open("enum Palette")
for (key, token) in light.members {
    out.doc(token["$description"]?.string)
    out.line(
        "static let \(lowerCamel(key)) = Swatch(light: \(colour(token, "color.light.\(key)")), "
            + "dark: \(colour(dark[key], "color.dark.\(key)")))")
}
for (key, token) in colours.members where key != "light" && key != "dark" {
    out.doc(token["$description"]?.string)
    out.line("static let \(lowerCamel(key)) = Swatch(\(colour(token, "color.\(key)")))")
}
out.close()

// Accents.
let accents = section("accent")
out.line()
out.doc("An accent: its fill, the colour of text in it, and its tint.")
out.open("struct Accent")
let fillDoc = accents.members.first?.1["accent"]?["$description"]?.string
out.doc(fillDoc.map { "\($0) The same in both appearances." })
out.line("let fill: Swatch")
out.doc("Text and icons in the accent.")
out.line("let text: Swatch")
out.doc("A wash of the accent, under selected rows and tinted buttons.")
out.line("let tint: Swatch")
for (key, accent) in accents.members {
    let what = "accent.\(key)"
    out.line()
    out.line("static let \(lowerCamel(key)) = Accent(")
    out.depth += 1
    out.line("fill: Swatch(\(colour(accent["accent"], "\(what).accent"))),")
    for part in ["text", "tint"] {
        let pair = accent[part]
        let comma = part == "tint" ? "" : ","
        out.line(
            "\(part): Swatch(light: \(colour(pair?["light"], "\(what).\(part).light")), "
                + "dark: \(colour(pair?["dark"], "\(what).\(part).dark")))\(comma)")
    }
    out.depth -= 1
    out.line(")")
}
out.line()
out.doc("Every accent, in the order the tokens list them.")
let accentNames = accents.members.map { lowerCamel($0.0) }.joined(separator: ", ")
out.line("static let all: [Accent] = [\(accentNames)]")
out.close()

// Avatars.
let avatars = section("avatar")
guard let lightPairs = avatars["light"], let darkPairs = avatars["dark"],
    lightPairs.members.map({ $0.0 }) == darkPairs.members.map({ $0.0 })
else { fail("avatar needs the same pairs in light and dark") }
out.line()
out.doc("An avatar's colours: its ground and its initials. " + (avatars["$description"]?.string ?? ""))
out.open("struct AvatarPair")
out.line("let background: Swatch")
out.line("let initials: Swatch")
out.line()
out.doc("Every pair, in order.")
out.line("static let all: [AvatarPair] = [")
out.depth += 1
for (key, pair) in lightPairs.members {
    let other = darkPairs[key]
    let what = "avatar.*.\(key)"
    out.line("AvatarPair(")
    out.depth += 1
    out.line(
        "background: Swatch(light: \(colour(pair["background"], "\(what).background")), "
            + "dark: \(colour(other?["background"], "\(what).background"))),")
    out.line(
        "initials: Swatch(light: \(colour(pair["initials"], "\(what).initials")), "
            + "dark: \(colour(other?["initials"], "\(what).initials")))")
    out.depth -= 1
    out.line("),")
}
out.depth -= 1
out.line("]")
out.close()
out.close()

// Settings: whatever has values under each setting, as nested namespaces.
// Text size is the type scale's, above; settings described only in words
// (the typeface, the tinted sidebar, the buttons) have nothing to generate.
func hasValues(_ node: JSON) -> Bool {
    node.isToken || node.members.contains { hasValues($0.1) }
}

func settingsGroup(_ key: String, _ node: JSON, path: String) {
    out.line()
    out.doc(node["$description"]?.string)
    out.open("enum \(upperCamel(key))")
    for (child, value) in node.members where hasValues(value) {
        let what = "\(path).\(child)"
        if value.isToken {
            out.doc(value["$description"]?.string)
            switch value["$type"]?.string {
            case "dimension":
                out.line("static let \(lowerCamel(child)): CGFloat = \(points(value["$value"], what))")
            case "color":
                out.line("static let \(lowerCamel(child)) = Swatch(\(colour(value, what)))")
            default:
                fail("\(what) is neither a size nor a colour")
            }
        } else {
            settingsGroup(child, value, path: what)
        }
    }
    out.close()
}

out.line()
out.line("// MARK: - Settings")
out.line()
out.open("extension Tokens")
let settings = section("settings")
out.doc(settings["$description"]?.string)
out.open("enum Settings")
for (key, node) in settings.members where key != "text-size" && hasValues(node) {
    settingsGroup(key, node, path: "settings.\(key)")
}
out.close()
out.close()

// One blank line between a closing brace and what follows it, none after an
// opening one, and none doubled.
var tidy: [String] = []
for line in out.lines {
    if line.isEmpty, tidy.last?.isEmpty ?? true { continue }
    if line.isEmpty, tidy.last?.hasSuffix("{") == true { continue }
    tidy.append(line)
}
while tidy.last?.isEmpty == true { tidy.removeLast() }

// Written formatted rather than formatted afterwards (see the top), so a
// line `swift format` would break is refused here, not left for the lint.
if let long = tidy.firstIndex(where: { $0.count > 100 }) {
    fail("line \(long + 1) would be over 100 columns: \(tidy[long])")
}
let text = tidy.joined(separator: "\n") + "\n"
do {
    try FileManager.default.createDirectory(
        at: URL(fileURLWithPath: destination).deletingLastPathComponent(),
        withIntermediateDirectories: true)
    try text.write(toFile: destination, atomically: true, encoding: .utf8)
} catch {
    fail("cannot write \(destination): \(error)")
}
