import Foundation
import Testing
@testable import Scufris

struct MarkdownDetailsTests {
    @Test
    func detailsHaveTheSupportedBlockHierarchy() {
        let source = """
        # Result

        A paragraph with **strong**, *emphasis*, `code`, and https://example.com/docs.

        - one
        - two

        3. third
        4. fourth

        > quoted **text**

        ---

        ```swift
        let answer = 42
        ```
        """
        #expect(
            MarkdownBlockParser.parse(source) == [
                .heading(level: 1, text: "Result"),
                .paragraph(
                    "A paragraph with **strong**, *emphasis*, `code`, and https://example.com/docs."
                ),
                .unorderedList(["one", "two"]),
                .orderedList(start: 3, items: ["third", "fourth"]),
                .quote("quoted **text**"),
                .thematicRule,
                .code(language: "swift", text: "let answer = 42"),
            ]
        )
    }

    @Test
    func plainResponseTextStaysLiteralButSafeBareURLsBecomeLinks() {
        let source = "# literal **strong** `code` https://example.com/report?x=1"
        let rendered = ConversationMarkup.plain(source)
        #expect(String(rendered.characters) == source)
        #expect(
            rendered.runs.compactMap(\.link).map(\.absoluteString)
                == ["https://example.com/report?x=1"]
        )
        #expect(
            rendered.runs.allSatisfy { $0.inlinePresentationIntent == nil }
        )
    }

    @Test
    func detailsInterpretInlineMarkdownOnlyInsideTheMarkdownBoundary() {
        let rendered = ConversationMarkup.markdown(
            "**strong** and *emphasis* with `code` and [safe](https://example.com)."
        )
        #expect(
            rendered.runs.contains {
                $0.inlinePresentationIntent?.contains(.stronglyEmphasized) == true
            }
        )
        #expect(
            rendered.runs.contains {
                $0.inlinePresentationIntent?.contains(.emphasized) == true
            }
        )
        #expect(
            rendered.runs.contains {
                $0.inlinePresentationIntent?.contains(.code) == true
            }
        )
        #expect(rendered.runs.compactMap(\.link).count == 1)
    }

    @Test
    func unsafeLinksAndRawHTMLStayInert() {
        let rendered = ConversationMarkup.markdown(
            "[script](javascript:alert(1)) [file](file:///etc/passwd) <script src=https://evil.test>x</script>"
        )
        #expect(rendered.runs.compactMap(\.link).isEmpty)
        #expect(
            MarkdownBlockParser.parse("<script>alert('x')</script>")
                == [.paragraph("<script>alert('x')</script>")]
        )

        for value in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "mailto:person@example.com",
            "https://user:secret@example.com/",
            "https:///missing-host",
        ] {
            #expect(ConversationLink.safe(URL(string: value)) == nil)
        }
        #expect(ConversationLink.safe(URL(string: "https://example.com/path")) != nil)
    }

    @Test
    func absentAndMalformedDetailsRemainSafe() {
        #expect(conversationDetailsAreValid(nil))
        #expect(!conversationDetailsAreValid(""))
        #expect(!conversationDetailsAreValid("  \n"))
        #expect(!conversationDetailsAreValid("bad\rline"))
        #expect(!conversationDetailsAreValid(String(repeating: "x", count: 32 * 1024 + 1)))

        let malformed = "[unfinished **strong"
        #expect(MarkdownBlockParser.parse(malformed) == [.paragraph(malformed)])
        #expect(String(ConversationMarkup.markdown(malformed).characters) == malformed)
    }
}
