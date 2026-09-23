//! Rust-side (tray menu) strings. The page has its own dictionary; keys are kept identical on both sides.

pub fn resolve_auto() -> &'static str {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        let n = GetUserDefaultLocaleName(&mut buf);
        if n > 0 {
            let name = String::from_utf16_lossy(&buf[..(n as usize - 1)]).to_lowercase();
            if name.starts_with("zh") {
                return "zh";
            }
            if name.starts_with("ja") {
                return "ja";
            }
            if name.starts_with("ko") {
                return "ko";
            }
            if name.starts_with("ar") {
                return "ar";
            }
        }
    }
    "en"
}

pub fn tr(lang: &str, key: &str) -> &'static str {
    let l = if lang == "auto" { resolve_auto() } else { lang };
    match (l, key) {
        ("ar", "install") => "تثبيت روابط Claude Code",
        ("ar", "uninstall") => "إزالة الروابط",
        ("ar", "language") => "اللغة",
        ("ar", "lang_auto") => "حسب النظام",
        ("ar", "reset_pos") => "إعادة موضع الشريط",
        ("ar", "quit") => "خروج",
        ("ar", "hooks_missing") => "الروابط غير مثبتة: انقر بزر الفأرة الأيمن على أيقونة الدرج ← تثبيت روابط Claude Code",
        ("ar", "autostart") => "التشغيل مع Windows (بصمت)",
        ("ar", "refresh") => "تحديث الاستخدام الآن",
        ("ar", "open_data") => "فتح مجلد البيانات (السجلات / الأيقونات)",
        ("ar", "hover_only") => "إظهار الشريط عند المرور بالمؤشر فقط",
        ("ar", "pplx_connect") => "ربط Perplexity…",
        ("ar", "pplx_disconnect") => "فصل Perplexity",
        ("ar", "settings") => "الإعدادات…",
        ("ar", "install_update") => "تثبيت التحديث",
        ("zh", "install") => "安装 Claude Code 钩子",
        ("zh", "uninstall") => "卸载钩子",
        ("zh", "language") => "语言",
        ("zh", "lang_auto") => "跟随系统",
        ("zh", "reset_pos") => "重置悬浮条位置",
        ("zh", "quit") => "退出",
        ("zh", "hooks_missing") => "钩子未安装：右键托盘图标 → 安装 Claude Code 钩子（桌面版无需，已自动兜底）",
        ("zh", "autostart") => "开机自启（静默待命）",
        ("ja", "autostart") => "Windows起動時に自動開始",
        ("ko", "autostart") => "Windows 시작 시 자동 실행",
        ("zh", "refresh") => "立即刷新用量",
        ("zh", "open_data") => "打开数据文件夹（日志 / 图标）",
        ("ja", "open_data") => "データフォルダを開く（ログ / アイコン）",
        ("ko", "open_data") => "데이터 폴더 열기 (로그 / 아이콘)",
        (_, "open_data") => "Open data folder (logs / icons)",
        ("ja", "refresh") => "使用量を今すぐ更新",
        ("ko", "refresh") => "사용량 지금 새로고침",
        ("ja", "install") => "Claude Code フックを導入",
        ("ja", "uninstall") => "フックを削除",
        ("ja", "language") => "言語",
        ("ja", "lang_auto") => "システムに従う",
        ("ja", "reset_pos") => "バー位置をリセット",
        ("ja", "quit") => "終了",
        ("ja", "hooks_missing") => "フック未導入：トレイ右クリック → フックを導入（デスクトップ版は自動フォールバック済み）",
        ("ko", "install") => "Claude Code 후크 설치",
        ("ko", "uninstall") => "후크 제거",
        ("ko", "language") => "언어",
        ("ko", "lang_auto") => "시스템 따르기",
        ("ko", "reset_pos") => "바 위치 초기화",
        ("ko", "quit") => "종료",
        ("ko", "hooks_missing") => "후크 미설치: 트레이 우클릭 → 후크 설치 (데스크톱판은 자동 폴백)",
        (_, "install") => "Install Claude Code hooks",
        (_, "uninstall") => "Uninstall hooks",
        (_, "language") => "Language",
        (_, "lang_auto") => "Follow system",
        (_, "reset_pos") => "Reset bar position",
        (_, "quit") => "Quit",
        (_, "hooks_missing") => "Hooks not installed: tray right-click → Install Claude Code hooks (desktop app auto-fallback active)",
        (_, "autostart") => "Start with Windows (silent)",
        (_, "refresh") => "Refresh usage now",
        ("zh", "hover_only") => "仅在鼠标悬停时显示",
        ("ja", "hover_only") => "ホバー時のみ表示",
        ("ko", "hover_only") => "마우스를 올릴 때만 표시",
        (_, "hover_only") => "Show notch only on hover",
        ("zh", "pplx_connect") => "连接 Perplexity…",
        ("ja", "pplx_connect") => "Perplexity に接続…",
        ("ko", "pplx_connect") => "Perplexity 연결…",
        (_, "pplx_connect") => "Connect Perplexity…",
        ("zh", "pplx_disconnect") => "断开 Perplexity",
        ("ja", "pplx_disconnect") => "Perplexity の接続を解除",
        ("ko", "pplx_disconnect") => "Perplexity 연결 해제",
        (_, "pplx_disconnect") => "Disconnect Perplexity",
        ("zh", "settings") => "设置…",
        ("ja", "settings") => "設定…",
        ("ko", "settings") => "설정…",
        (_, "settings") => "Settings…",
        ("zh", "install_update") => "安装更新",
        ("ja", "install_update") => "アップデートをインストール",
        ("ko", "install_update") => "업데이트 설치",
        (_, "install_update") => "Install update",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    /// Keys defined in one language block of the page dictionary (`name:'…'` pairs)
    fn keys(block: &str) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        for (i, _) in block.match_indices(":'") {
            let id: String = block[..i].chars().rev().take_while(|c| c.is_ascii_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
            if !id.is_empty() {
                out.insert(id);
            }
        }
        out
    }

    /// Arabic is a first-class language: every tray string has its own Arabic text, none falls back
    #[test]
    fn every_tray_string_has_arabic() {
        for k in ["install", "uninstall", "language", "lang_auto", "reset_pos", "quit", "hooks_missing", "autostart", "refresh", "open_data", "hover_only", "pplx_connect", "pplx_disconnect", "settings", "install_update"] {
            let (ar, en) = (super::tr("ar", k), super::tr("en", k));
            assert_ne!(ar, "?", "{k}");
            assert_ne!(ar, en, "{k} has no Arabic text");
        }
    }


    const PAGE: &str = include_str!("../ui/notch.html");
    const NOTES_JS: &str = include_str!("../ui/notes.js");
    const SETTINGS: &str = include_str!("../ui/settings.html");

    /// One language block of a `const NAME={ xx:{…}, … };` dictionary
    fn block_of(src: &str, name: &str, lang: &str) -> String {
        let start = src.find(&format!("const {name}={{")).unwrap_or_else(|| panic!("{name} missing"));
        let body = &src[start..start + src[start..].find("\n};").expect("end")];
        let a = body.find(&format!("\n  {lang}:{{")).unwrap_or_else(|| panic!("{name}.{lang} missing"));
        let rest = &body[a + 3..];
        let b = rest.find("\n  ").map(|i| {
            // the next language header starts a line with two spaces and `xx:{`
            let mut j = i;
            while let Some(k) = rest[j + 1..].find("\n  ") {
                let line = &rest[j + 1 + k + 3..];
                if line.len() > 3 && line.as_bytes()[2] == b':' && line.as_bytes()[3] == b'{' {
                    return j + 1 + k;
                }
                j += 1 + k;
            }
            rest.len()
        });
        rest[..b.unwrap_or(rest.len())].to_string()
    }

    fn block(lang: &str) -> String {
        block_of(PAGE, "STR", lang)
    }

    /// The text of `key` in a language block (`key:'…'`)
    fn value(block: &str, key: &str) -> Option<String> {
        let pat = format!("{key}:'");
        let mut from = 0;
        while let Some(i) = block[from..].find(&pat) {
            let at = from + i;
            // a whole key: not the tail of a longer identifier
            let whole = block[..at].chars().last().map(|c| !c.is_ascii_alphanumeric()).unwrap_or(true);
            let v = &block[at + pat.len()..];
            if whole {
                return v.find('\'').map(|e| v[..e].to_string());
            }
            from = at + pat.len();
        }
        None
    }

    /// Every card string exists in every language the tray offers (W-13)
    #[test]
    fn the_page_dictionary_is_complete_in_every_language() {
        let en = keys(&block("en"));
        assert!(en.len() > 20, "{en:?}");
        for l in ["ar", "zh", "ja", "ko"] {
            assert_eq!(keys(&block(l)), en, "{l} does not define the same strings as en");
        }
    }

    /// Provider notes (W-23): every code Rust can emit is in notes.js with the English text equal to
    /// the Rust template (the pages check their English rendering against `note` before translating)
    /// and its own Arabic with the same placeholders; notes.js has no code Rust does not know.
    /// zh/ja/ko are not required: they fall back to English.
    #[test]
    fn every_note_code_is_translated_and_matches_the_rust_template() {
        let (en, ar) = (block_of(NOTES_JS, "NOTE_STR", "en"), block_of(NOTES_JS, "NOTE_STR", "ar"));
        for (code, tpl) in crate::notes::EN {
            assert_eq!(value(&en, code).as_deref(), Some(*tpl), "en text of {code}");
            let a = value(&ar, code).unwrap_or_else(|| panic!("{code} has no Arabic"));
            assert!(!a.trim().is_empty() && a != *tpl, "{code} is not translated to Arabic");
            for i in 0..4 {
                let ph = format!("{{{i}}}");
                assert_eq!(tpl.contains(&ph), a.contains(&ph), "{code}: placeholder {ph} differs in Arabic");
            }
        }
        let known: std::collections::BTreeSet<String> = crate::notes::EN.iter().map(|(c, _)| c.to_string()).collect();
        assert_eq!(keys(&en), known, "notes.js en and src/notes.rs define different codes");
        assert_eq!(keys(&ar), known, "notes.js ar and src/notes.rs define different codes");
    }

    /// Both pages load the shared note dictionary and translate notes with it (W-23, W-26); the
    /// card merges it into STR, and the note codes never collide with a card string
    #[test]
    fn both_pages_translate_notes_with_the_shared_dictionary() {
        for (name, page) in [("notch.html", PAGE), ("settings.html", SETTINGS)] {
            assert!(page.contains("<script src=\"notes.js\"></script>"), "{name} does not load notes.js");
            assert!(page.contains("localNote("), "{name} does not translate notes");
        }
        assert!(PAGE.contains("for(const l in NOTE_STR) STR[l]="), "the card does not merge the notes into STR");
        let card = keys(&block("en"));
        for (code, _) in crate::notes::EN {
            assert!(!card.contains(*code), "{code} collides with a card string");
        }
    }

    /// The fixed window labels the adapters produce have Arabic text in the page's LABELS (numbered
    /// ones like "3h limit" or "Usage (2 wk)" are handled by LABEL_RULES)
    #[test]
    fn fixed_window_labels_have_arabic() {
        let start = PAGE.find("const LABELS={").expect("labels");
        let ar = &PAGE[start..];
        let ar = &ar[ar.find("\n  ar:{").expect("ar labels")..ar.find("\n  zh:{").expect("zh labels")];
        for l in [
            "Current session", "Weekly (all models)", "Weekly (Opus)", "Weekly (model-scoped)", "Weekly", "Weekly limit",
            "Monthly limit", "5h limit", "Tokens this month", "Tokens today", "Included usage", "API usage", "On demand",
            "Premium requests", "Chat requests", "Completions", "Pro searches", "Research", "Agentic research", "Labs",
            "Free queries", "Session usage", "Weekly usage", "Monthly usage", "MCP (1 month)", "Usage",
            "Requests today · no limit published",
        ] {
            assert!(ar.contains(&format!("'{l}':'")), "no Arabic for the label {l:?}");
        }
        for rule in [r"(\d+)m limit", r"(\d+)h limit", r"(\d+)d limit", r"Usage \((\d+) h\)", r"Usage \((\d+) wk\)", "(.+) · this month"] {
            assert!(PAGE.contains(&format!("/^{rule}$/")), "no Arabic rule for {rule}");
        }
    }
}
