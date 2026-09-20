import XCTest
@testable import Codenotch

/// Catalog lookups with an explicit locale. English is the source; Chinese
/// and French assertions here only prove a translation that exists is served,
/// not that every key has one.
final class LocalizationTests: XCTestCase {
    private let zhHans = Locale(identifier: "zh-Hans")
    private let french = Locale(identifier: "fr")
    private let english = Locale(identifier: "en")
    private let now = Date(timeIntervalSince1970: 1_787_900_000)
    private let resetNow = Date(timeIntervalSince1970: 1_700_000_000)

    // MARK: - ElapsedCopy

    func testElapsedCopyInSimplifiedChinese() {
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-5), now: now, locale: zhHans),
            "刚刚"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-6 * 60), now: now, locale: zhHans),
            "6 分钟"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-60 * 60), now: now, locale: zhHans),
            "1 小时"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-65 * 60), now: now, locale: zhHans),
            "1 小时 5 分钟"
        )
        XCTAssertEqual(
            ElapsedCopy.ago(since: now.addingTimeInterval(-6 * 60), now: now, locale: zhHans),
            "6 分钟前"
        )
    }

    func testElapsedCopyInEnglishWhenAsked() {
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-5), now: now, locale: english),
            "just now"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-6 * 60), now: now, locale: english),
            "6 min"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-60 * 60), now: now, locale: english),
            "1 hr"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-65 * 60), now: now, locale: english),
            "1 hr 5 min"
        )
        XCTAssertEqual(
            ElapsedCopy.ago(since: now.addingTimeInterval(-6 * 60), now: now, locale: english),
            "6 min ago"
        )
    }

    // MARK: - ResetCopy

    func testResetCopyUnderAnHourInSimplifiedChinese() {
        XCTAssertEqual(
            ResetCopy.text(for: resetNow.addingTimeInterval(51 * 60), now: resetNow, locale: zhHans),
            "51 分钟后重置"
        )
        XCTAssertEqual(
            ResetCopy.text(for: resetNow.addingTimeInterval(-5), now: resetNow, locale: zhHans),
            "正在重置…"
        )
    }

    func testResetCopyUnderAnHourInEnglishWhenAsked() {
        XCTAssertEqual(
            ResetCopy.text(for: resetNow.addingTimeInterval(51 * 60), now: resetNow, locale: english),
            "Resets in 51 min"
        )
        XCTAssertEqual(
            ResetCopy.text(for: resetNow.addingTimeInterval(-5), now: resetNow, locale: english),
            "Resetting…"
        )
    }

    // MARK: - LimitWindow.summary

    func testWindowSummaryInSimplifiedChinese() {
        XCTAssertEqual(
            percentWindow(0.12).summary(locale: zhHans),
            "12% 已用 · 88% 剩余"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests", used: 8).summary(locale: zhHans),
            "已用 8"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests", remaining: 3).summary(locale: zhHans),
            "剩余 3"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests").summary(locale: zhHans),
            "暂无读数"
        )
    }

    func testWindowSummaryInEnglishWhenAsked() {
        XCTAssertEqual(
            percentWindow(0.12).summary(locale: english),
            "12% Used · 88% left"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests", used: 8).summary(locale: english),
            "8 used"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests", remaining: 3).summary(locale: english),
            "3 left"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests").summary(locale: english),
            "No reading"
        )
    }

    // MARK: - Menu and settings keys

    func testMenuCopyInSimplifiedChinese() {
        XCTAssertEqual(L10n.t("Always show", locale: zhHans), "始终显示")
        XCTAssertEqual(L10n.t("Settings…", locale: zhHans), "设置…")
    }

    func testMenuCopyInEnglishWhenAsked() {
        XCTAssertEqual(L10n.t("Always show", locale: english), "Always show")
        XCTAssertEqual(L10n.t("Settings…", locale: english), "Settings…")
    }

    // MARK: - Sign-in

    func testSignInActionTitleStaysEnglishUnderTheTestPin() {
        XCTAssertEqual(
            SignInRoute.modal(name: "Perplexity").actionTitle,
            "Sign in to Perplexity"
        )
    }

    func testSignInCopyInSimplifiedChinese() {
        XCTAssertEqual(
            L10n.t("Sign in to \("Perplexity")", locale: zhHans),
            "登录 Perplexity"
        )
    }

    func testSignInCopyInEnglishWhenAsked() {
        XCTAssertEqual(
            L10n.t("Sign in to \("Perplexity")", locale: english),
            "Sign in to Perplexity"
        )
    }

    // MARK: - French

    func testElapsedCopyInFrench() {
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-5), now: now, locale: french),
            "à l'instant"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-6 * 60), now: now, locale: french),
            "6 min"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-60 * 60), now: now, locale: french),
            "1 h"
        )
        XCTAssertEqual(
            ElapsedCopy.text(since: now.addingTimeInterval(-65 * 60), now: now, locale: french),
            "1 h 5 min"
        )
        XCTAssertEqual(
            ElapsedCopy.ago(since: now.addingTimeInterval(-6 * 60), now: now, locale: french),
            "il y a 6 min"
        )
    }

    func testResetCopyUnderAnHourInFrench() {
        XCTAssertEqual(
            ResetCopy.text(for: resetNow.addingTimeInterval(51 * 60), now: resetNow, locale: french),
            "Réinit. dans 51 min"
        )
        XCTAssertEqual(
            ResetCopy.text(for: resetNow.addingTimeInterval(-5), now: resetNow, locale: french),
            "Réinitialisation…"
        )
    }

    func testWindowSummaryInFrench() {
        XCTAssertEqual(
            percentWindow(0.12).summary(locale: french),
            "12% utilisés · 88% restants"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests", used: 8).summary(locale: french),
            "8 utilisés"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests", remaining: 3).summary(locale: french),
            "3 restants"
        )
        XCTAssertEqual(
            LimitWindow(id: "w", label: "Requests").summary(locale: french),
            "Aucun relevé"
        )
    }

    func testMenuCopyInFrench() {
        XCTAssertEqual(L10n.t("Always show", locale: french), "Toujours afficher")
        XCTAssertEqual(L10n.t("Settings…", locale: french), "Réglages…")
    }

    func testSignInCopyInFrench() {
        XCTAssertEqual(
            L10n.t("Sign in to \("Perplexity")", locale: french),
            "Se connecter à Perplexity"
        )
    }

    /// Every language the picker offers must resolve to a locale the catalog
    /// is filed under — a region-qualified or unshipped identifier silently
    /// serves another language instead.
    func testEveryOfferedLanguageResolves() {
        XCTAssertEqual(
            AppLanguage.allCases.map(\.rawValue),
            ["system", "en", "fr", "zh-Hans"]
        )
        XCTAssertNil(AppLanguage.system.locale)
        for language in AppLanguage.allCases where language != .system {
            XCTAssertEqual(language.locale?.identifier, language.rawValue)
        }
    }

    private func percentWindow(_ fraction: Double) -> LimitWindow {
        LimitWindow(id: "w", label: "Monthly limit", usedFraction: fraction)
    }
}
