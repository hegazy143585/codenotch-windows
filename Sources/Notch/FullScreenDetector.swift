import AppKit
import CoreGraphics

/// Detects whether a full-screen application window is active on a given display.
enum FullScreenDetector {
    /// Pure function checking whether any layer 0 window belonging to `frontmostPID`
    /// matches or spans the `screenBounds`.
    static func isFullScreen(
        screenBounds: CGRect,
        frontmostPID: pid_t,
        windows: [(pid: pid_t, layer: Int, bounds: CGRect)]
    ) -> Bool {
        for window in windows {
            guard window.pid == frontmostPID, window.layer == 0 else { continue }
            if abs(window.bounds.width - screenBounds.width) <= 2 &&
               abs(window.bounds.height - screenBounds.height) <= 2 {
                return true
            }
        }
        return false
    }

    /// Queries WindowServer and NSWorkspace to determine if the frontmost app
    /// is occupying the entire `screen`.
    static func isFullScreenAppFrontmost(on screen: NSScreen? = NSScreen.main) -> Bool {
        guard let screen = screen ?? NSScreen.main else { return false }
        guard let frontApp = NSWorkspace.shared.frontmostApplication else { return false }

        // Ignore Codenotch itself (settings panel, etc.)
        guard frontApp.bundleIdentifier != Bundle.main.bundleIdentifier else { return false }

        if let windowInfoList = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
            var extractedWindows: [(pid: pid_t, layer: Int, bounds: CGRect)] = []
            for info in windowInfoList {
                guard let pid = info[kCGWindowOwnerPID as String] as? pid_t,
                      let layer = info[kCGWindowLayer as String] as? Int,
                      let boundsDict = info[kCGWindowBounds as String] as? NSDictionary,
                      let bounds = CGRect(dictionaryRepresentation: boundsDict as CFDictionary)
                else { continue }
                extractedWindows.append((pid: pid, layer: layer, bounds: bounds))
            }

            if isFullScreen(
                screenBounds: screen.frame,
                frontmostPID: frontApp.processIdentifier,
                windows: extractedWindows
            ) {
                return true
            }
        }

        // Supplementary check: on a full-screen Space, macOS autohides the menu bar,
        // causing visibleFrame to match the entire screen frame.
        if abs(screen.visibleFrame.width - screen.frame.width) <= 1 &&
           abs(screen.visibleFrame.height - screen.frame.height) <= 1 &&
           frontApp.bundleIdentifier != "com.apple.finder" {
            return true
        }

        return false
    }
}
