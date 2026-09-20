import XCTest
@testable import Codenotch

/// In-app language is a stored override, not the Mac's language. Follow
/// System still hits the XCTest English pin when nothing is stored.
final class AppLanguageTests: XCTestCase {
    private var suiteName = ""
    private var previousDefaults: UserDefaults?
    private var previousTestLocale: Locale?

    /// A scratch suite, not `.standard`. The test host *is* the app, so
    /// `.standard` is the preferences of the copy of Codenotch installed on
    /// this Mac: reading it would let a language chosen in Settings decide
    /// what these assert, and writing it would leave a language behind in the
    /// real app when a test failed before its restore.
    override func setUp() {
        super.setUp()
        suiteName = "AppLanguageTests.\(UUID().uuidString)"
        let scratch = UserDefaults(suiteName: suiteName)!
        scratch.removePersistentDomain(forName: suiteName)
        previousDefaults = L10n.defaults
        previousTestLocale = L10n.testLocale
        L10n.defaults = scratch
        L10n.testLocale = nil
        L10n.apply(.system)
    }

    override func tearDown() {
        L10n.testLocale = previousTestLocale
        L10n.defaults.removePersistentDomain(forName: suiteName)
        if let previousDefaults { L10n.defaults = previousDefaults }
        super.tearDown()
    }

    func testFollowSystemUsesTheEnglishPinWhenNothingIsStored() {
        L10n.apply(.system)
        L10n.testLocale = nil
        XCTAssertTrue(
            L10n.locale.identifier.hasPrefix("en"),
            "XCTest pin should return English when appLanguage is unset, got \(L10n.locale.identifier)"
        )
    }

    /// A forced English must actually be English. The catalog files its
    /// source strings under `en`, so the region-qualified `en_US` this used
    /// to store matched nothing and fell through to the next localization the
    /// bundle offered — Chinese, on a build that ships one.
    func testApplyEnglishServesEnglishCopy() {
        L10n.apply(.english)
        L10n.testLocale = nil
        XCTAssertEqual(L10n.t("Always show"), "Always show")
    }

    /// `apply(.simplifiedChinese)` stores `zh-Hans`, and `L10n.locale`
    /// honours that even under XCTest, so the default `t()` lookup is
    /// Chinese without setting `testLocale`.
    func testApplySimplifiedChineseServesChineseCopy() {
        L10n.apply(.simplifiedChinese)
        L10n.testLocale = nil
        XCTAssertEqual(L10n.t("Always show"), "始终显示")
    }
}
