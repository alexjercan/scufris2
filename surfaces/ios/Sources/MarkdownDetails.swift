import Foundation
import SwiftUI

/// The only external destinations conversation content may open.
enum ConversationLink {
    static let maximumBytes = 8 * 1024

    static func safe(_ candidate: URL?) -> URL? {
        guard let candidate else { return nil }
        let absolute = candidate.absoluteString
        guard
            absolute.utf8.count <= maximumBytes,
            !absolute.unicodeScalars.contains(where: {
                CharacterSet.controlCharacters.contains($0)
            }),
            let authority = absolute.range(of: "://"),
            !absolute[authority.upperBound...].hasPrefix("/"),
            let components = URLComponents(url: candidate, resolvingAgainstBaseURL: false),
            let scheme = components.scheme?.lowercased(),
            scheme == "http" || scheme == "https",
            let host = components.host,
            !host.isEmpty,
            components.user == nil,
            components.password == nil
        else {
            return nil
        }
        return components.url
    }
}

/// Native inline rendering shared by plain conversation text and Markdown
/// blocks. Plain text starts as a literal attributed string. Details use
/// Foundation's Markdown parser. Both paths add only safe detected web URLs.
enum ConversationMarkup {
    static func plain(_ source: String) -> AttributedString {
        var result = AttributedString(source)
        addBareLinks(to: &result, excludingRawHTML: false)
        return result
    }

    static func markdown(_ source: String) -> AttributedString {
        var result = (try? AttributedString(
            markdown: source,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(source)
        sanitizeMarkdownLinks(in: &result)
        addBareLinks(to: &result, excludingRawHTML: true)
        styleCode(in: &result)
        return result
    }

    private static func sanitizeMarkdownLinks(in result: inout AttributedString) {
        let links: [(Range<AttributedString.Index>, URL?)] = result.runs.compactMap { run in
            guard let current = run.link else { return nil }
            return (run.range, ConversationLink.safe(current))
        }
        for (range, safe) in links {
            result[range].link = safe
        }
    }

    private static func addBareLinks(
        to result: inout AttributedString,
        excludingRawHTML: Bool
    ) {
        guard
            let detector = try? NSDataDetector(
                types: NSTextCheckingResult.CheckingType.link.rawValue
            )
        else {
            return
        }
        let rendered = String(result.characters)
        let matches = detector.matches(
            in: rendered,
            range: NSRange(rendered.startIndex ..< rendered.endIndex, in: rendered)
        )
        for match in matches {
            guard
                let safe = ConversationLink.safe(match.url),
                let stringRange = Range(match.range, in: rendered)
            else {
                continue
            }
            if excludingRawHTML, isInsideRawHTML(stringRange, in: rendered) {
                continue
            }
            let lowerOffset = rendered[..<stringRange.lowerBound].count
            let upperOffset = rendered[..<stringRange.upperBound].count
            let lower = result.characters.index(
                result.characters.startIndex,
                offsetBy: lowerOffset
            )
            let upper = result.characters.index(
                result.characters.startIndex,
                offsetBy: upperOffset
            )
            let range = lower ..< upper
            let isCodeOrLink = result[range].runs.contains { run in
                run.link != nil || run.inlinePresentationIntent?.contains(.code) == true
            }
            if !isCodeOrLink {
                result[range].link = safe
            }
        }
    }

    private static func isInsideRawHTML(
        _ range: Range<String.Index>,
        in source: String
    ) -> Bool {
        let before = source[..<range.lowerBound]
        guard let opening = before.lastIndex(of: "<") else { return false }
        if let closing = before.lastIndex(of: ">"), closing > opening { return false }
        return source[range.upperBound...].contains(">")
    }

    private static func styleCode(in result: inout AttributedString) {
        let ranges = result.runs.compactMap { run in
            run.inlinePresentationIntent?.contains(.code) == true ? run.range : nil
        }
        for range in ranges {
            result[range].foregroundColor = ScufrisPalette.yellow
            result[range].backgroundColor = ScufrisPalette.line
        }
    }
}

enum MarkdownBlock: Equatable {
    case paragraph(String)
    case heading(level: Int, text: String)
    case unorderedList([String])
    case orderedList(start: Int, items: [String])
    case quote(String)
    case code(language: String?, text: String)
    case thematicRule
}

/// A bounded block parser around Foundation's native inline Markdown parser.
/// It recognizes only visible text structures and never creates HTML or remote
/// media views.
enum MarkdownBlockParser {
    static func parse(_ source: String) -> [MarkdownBlock] {
        let normalized = source
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
        var blocks: [MarkdownBlock] = []
        var index = 0

        while index < lines.count {
            let line = lines[index]
            if line.trimmingCharacters(in: .whitespaces).isEmpty {
                index += 1
                continue
            }

            if let opening = fence(line) {
                index += 1
                var body: [String] = []
                while index < lines.count,
                      !isClosingFence(lines[index], character: opening.character, width: opening.width)
                {
                    body.append(lines[index])
                    index += 1
                }
                if index < lines.count { index += 1 }
                blocks.append(.code(
                    language: opening.language.isEmpty ? nil : opening.language,
                    text: body.joined(separator: "\n")
                ))
                continue
            }

            if let value = heading(line) {
                blocks.append(.heading(level: value.level, text: value.text))
                index += 1
                continue
            }

            if thematic(line) {
                blocks.append(.thematicRule)
                index += 1
                continue
            }

            if quote(line) != nil {
                var body: [String] = []
                while index < lines.count, let value = quote(lines[index]) {
                    body.append(value)
                    index += 1
                }
                blocks.append(.quote(body.joined(separator: "\n")))
                continue
            }

            if let first = listItem(line) {
                var items: [String] = []
                let ordered = first.ordered
                let start = first.start
                while index < lines.count,
                      let item = listItem(lines[index]),
                      item.ordered == ordered
                {
                    items.append(item.text)
                    index += 1
                }
                blocks.append(
                    ordered
                        ? .orderedList(start: start, items: items)
                        : .unorderedList(items)
                )
                continue
            }

            var paragraph = [line.trimmingCharacters(in: .whitespaces)]
            index += 1
            while index < lines.count {
                let next = lines[index]
                if next.trimmingCharacters(in: .whitespaces).isEmpty || startsBlock(next) {
                    break
                }
                paragraph.append(next.trimmingCharacters(in: .whitespaces))
                index += 1
            }
            blocks.append(.paragraph(paragraph.joined(separator: "\n")))
        }
        return blocks
    }

    private struct Fence {
        let character: Character
        let width: Int
        let language: String
    }

    private struct Heading {
        let level: Int
        let text: String
    }

    private struct ListItem {
        let ordered: Bool
        let start: Int
        let text: String
    }

    private static func fence(_ line: String) -> Fence? {
        guard let fields = captures(#"^ {0,3}(`{3,}|~{3,})[ \t]*([^ \t`]*)[ \t]*$"#, in: line),
              let marker = fields.first,
              let character = marker.first
        else {
            return nil
        }
        return Fence(
            character: character,
            width: marker.count,
            language: fields.count > 1 ? fields[1] : ""
        )
    }

    private static func isClosingFence(_ line: String, character: Character, width: Int) -> Bool {
        let leading = line.prefix { $0 == " " }.count
        guard leading <= 3 else { return false }
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        return trimmed.count >= width && trimmed.allSatisfy { $0 == character }
    }

    private static func heading(_ line: String) -> Heading? {
        guard let fields = captures(#"^ {0,3}(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$"#, in: line),
              fields.count == 2
        else {
            return nil
        }
        return Heading(level: fields[0].count, text: fields[1])
    }

    private static func thematic(_ line: String) -> Bool {
        matches(#"^ {0,3}(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$"#, line)
    }

    private static func quote(_ line: String) -> String? {
        captures(#"^ {0,3}>[ \t]?(.*)$"#, in: line)?.first
    }

    private static func listItem(_ line: String) -> ListItem? {
        if let fields = captures(#"^ {0,3}[-+*][ \t]+(.*)$"#, in: line),
           let text = fields.first
        {
            return ListItem(ordered: false, start: 1, text: text)
        }
        guard let fields = captures(#"^ {0,3}(\d{1,9})[.)][ \t]+(.*)$"#, in: line),
              fields.count == 2,
              let start = Int(fields[0])
        else {
            return nil
        }
        return ListItem(ordered: true, start: start, text: fields[1])
    }

    private static func startsBlock(_ line: String) -> Bool {
        fence(line) != nil
            || heading(line) != nil
            || thematic(line)
            || quote(line) != nil
            || listItem(line) != nil
    }

    private static func matches(_ pattern: String, _ source: String) -> Bool {
        guard let expression = try? NSRegularExpression(pattern: pattern) else { return false }
        return expression.firstMatch(
            in: source,
            range: NSRange(source.startIndex ..< source.endIndex, in: source)
        ) != nil
    }

    private static func captures(_ pattern: String, in source: String) -> [String]? {
        guard let expression = try? NSRegularExpression(pattern: pattern),
              let match = expression.firstMatch(
                  in: source,
                  range: NSRange(source.startIndex ..< source.endIndex, in: source)
              )
        else {
            return nil
        }
        return (1 ..< match.numberOfRanges).map { index in
            guard let range = Range(match.range(at: index), in: source) else { return "" }
            return String(source[range])
        }
    }
}

struct MarkdownDetails: View {
    let blocks: [MarkdownBlock]

    init(_ source: String) {
        blocks = MarkdownBlockParser.parse(source)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                MarkdownBlockView(block: block)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .textSelection(.enabled)
        .environment(\.openURL, OpenURLAction { url in
            ConversationLink.safe(url) == nil ? .discarded : .systemAction
        })
    }
}

private struct MarkdownBlockView: View {
    let block: MarkdownBlock
    @ScaledMetric(relativeTo: .footnote) private var detailSize: CGFloat = 11

    @ViewBuilder
    var body: some View {
        switch block {
        case let .paragraph(source):
            richText(source)
        case let .heading(level, source):
            Text(ConversationMarkup.markdown(source))
                .font(.system(
                    size: detailSize * headingScale(level),
                    weight: .bold,
                    design: .monospaced
                ))
                .foregroundStyle(level <= 2 ? ScufrisPalette.wisteria : ScufrisPalette.quartz)
                .tint(ScufrisPalette.niagara)
                .accessibilityAddTraits(.isHeader)
                .padding(.bottom, level == 1 ? 4 : 0)
                .overlay(alignment: .bottom) {
                    if level == 1 {
                        Rectangle().fill(ScufrisPalette.line).frame(height: 1)
                    }
                }
        case let .unorderedList(items):
            list(items: items, start: nil)
        case let .orderedList(start, items):
            list(items: items, start: start)
        case let .quote(source):
            richText(source)
                .foregroundStyle(ScufrisPalette.muted)
                .padding(.leading, 11)
                .overlay(alignment: .leading) {
                    Rectangle().fill(ScufrisPalette.niagara).frame(width: 2)
                }
        case let .code(language, source):
            VStack(alignment: .leading, spacing: 5) {
                if let language {
                    Text(language.uppercased())
                        .font(.system(size: detailSize * 0.78, weight: .bold, design: .monospaced))
                        .tracking(0.8)
                        .foregroundStyle(ScufrisPalette.quartz)
                }
                ScrollView(.horizontal) {
                    Text(verbatim: source)
                        .font(.system(size: detailSize, design: .monospaced))
                        .foregroundStyle(ScufrisPalette.yellow)
                        .padding(9)
                        .textSelection(.enabled)
                }
                .scrollIndicators(.visible)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(ScufrisPalette.line.opacity(0.24))
            .overlay(alignment: .leading) {
                Rectangle().fill(ScufrisPalette.quartz).frame(width: 2)
            }
        case .thematicRule:
            Rectangle()
                .fill(ScufrisPalette.line)
                .frame(maxWidth: .infinity, minHeight: 1, maxHeight: 1)
                .accessibilityHidden(true)
        }
    }

    private func richText(_ source: String) -> some View {
        Text(ConversationMarkup.markdown(source))
            .font(.system(size: detailSize, design: .monospaced))
            .foregroundStyle(ScufrisPalette.foreground)
            .tint(ScufrisPalette.niagara)
            .lineSpacing(3)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func headingScale(_ level: Int) -> CGFloat {
        switch level {
        case 1: 1.35
        case 2: 1.22
        case 3: 1.12
        default: 1
        }
    }

    private func list(items: [String], start: Int?) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(start.map { "\($0 + index)." } ?? "-")
                        .font(.system(size: detailSize, weight: .bold, design: .monospaced))
                        .foregroundStyle(ScufrisPalette.quartz)
                    richText(item)
                }
                .accessibilityElement(children: .combine)
            }
        }
        .padding(.leading, 5)
    }
}
