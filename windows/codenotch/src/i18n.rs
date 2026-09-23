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

    /// One language block of the page's `STR` dictionary
    fn block(lang: &str) -> String {
        let html = PAGE;
        let start = html.find("const STR={").expect("dictionary");
        let body = &html[start..start + html[start..].find("\n};").expect("end")];
        let a = body.find(&format!("\n  {lang}:{{")).unwrap_or_else(|| panic!("{lang} missing"));
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

    /// Provider-note keys (W-23) are `n` + an upper-case letter; the rest are card strings
    fn is_note_key(k: &str) -> bool {
        k.len() > 1 && k.starts_with('n') && k.as_bytes()[1].is_ascii_uppercase()
    }

    /// Arabic has every card string and every note (owner priority: Arabic + English). zh/ja/ko keep
    /// every card string but may leave provider notes to the English fallback; no language defines a
    /// key English lacks (W-13, W-23).
    #[test]
    fn the_page_dictionary_is_complete_in_every_language() {
        let en = keys(&block("en"));
        assert!(en.len() > 20, "{en:?}");
        assert_eq!(keys(&block("ar")), en, "ar does not define the same strings as en");
        let card: std::collections::BTreeSet<String> = en.iter().filter(|k| !is_note_key(k)).cloned().collect();
        for l in ["zh", "ja", "ko"] {
            let have = keys(&block(l));
            assert!(have.is_subset(&en), "{l} defines keys en lacks: {:?}", have.difference(&en).collect::<Vec<_>>());
            assert!(card.is_subset(&have), "{l} misses card strings: {:?}", card.difference(&have).collect::<Vec<_>>());
        }
    }

    /// Every note code Rust can emit is in the page: English identical to the Rust template (the
    /// page checks its English rendering against `note` before translating), Arabic its own text
    /// with the same placeholders. And the page has no note key Rust does not know.
    #[test]
    fn every_note_code_is_translated_and_matches_the_rust_template() {
        let (en, ar) = (block("en"), block("ar"));
        for (code, tpl) in crate::notes::EN {
            assert_eq!(value(&en, code).as_deref(), Some(*tpl), "en text of {code}");
            let a = value(&ar, code).unwrap_or_else(|| panic!("{code} has no Arabic"));
            assert!(!a.trim().is_empty() && a != *tpl, "{code} is not translated to Arabic");
            for i in 0..4 {
                let ph = format!("{{{i}}}");
                assert_eq!(tpl.contains(&ph), a.contains(&ph), "{code}: placeholder {ph} differs in Arabic");
            }
        }
        for k in keys(&en).into_iter().filter(|k| is_note_key(k)) {
            assert!(crate::notes::EN.iter().any(|(c, _)| *c == k), "page note {k} is not a Rust note code");
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
