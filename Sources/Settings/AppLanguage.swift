import Foundation

/// Which language Codenotch's own copy uses.
///
/// Follow System is the default. A forced English or Simplified Chinese
/// choice exists because the Mac's language is not always the one the
/// person wants this app in — bilingual machines, or a Mac in a language
/// we do not ship.
enum AppLanguage: String, CaseIterable, Identifiable {
    case system = "system"
    case english = "en"
    case french = "fr"
    case simplifiedChinese = "zh-Hans"

    var id: String { rawValue }

    /// `nil` means follow the Mac.
    ///
    /// Plain `en`, not `en_US`: these identifiers are looked up against the
    /// string catalog, whose English is filed under `en`. A region-qualified
    /// identifier misses it and falls through to whatever localization the
    /// bundle offers next — which made choosing English serve Chinese.
    var locale: Locale? {
        switch self {
        case .system:            return nil
        case .english:           return Locale(identifier: "en")
        case .french:            return Locale(identifier: "fr")
        case .simplifiedChinese: return Locale(identifier: "zh-Hans")
        }
    }

    /// English, Français and 简体中文 stay in their own language so the row
    /// is recognizable when the rest of Settings is in another one.
    var title: String {
        switch self {
        case .system:            return L10n.t("Follow System")
        case .english:           return "English"
        case .french:            return "Français"
        case .simplifiedChinese: return "简体中文"
        }
    }

    var explanation: String {
        switch self {
        case .system:
            return L10n.t("Matches the Mac's preferred language.")
        case .english, .french, .simplifiedChinese:
            return L10n.t("Codenotch uses this language even if the Mac does not.")
        }
    }
}
