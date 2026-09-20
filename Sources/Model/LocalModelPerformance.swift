import Foundation
import SwiftUI

/// The latest completed native Ollama response, not a quota or a PC health score.
struct LocalModelPerformance: Equatable {
    let outputTokens: Int
    let generationSeconds: TimeInterval
    let measuredAt: Date

    init?(outputTokens: Int, durationNanoseconds: Int64, measuredAt: Date = Date()) {
        guard outputTokens > 0, durationNanoseconds > 0 else { return nil }
        self.outputTokens = outputTokens
        generationSeconds = Double(durationNanoseconds) / 1_000_000_000
        self.measuredAt = measuredAt
    }

    var fidelity: Fidelity { .derived }
    var tokensPerSecond: Double { Double(outputTokens) / generationSeconds }
    var speedText: String {
        tokensPerSecond < 0.1 ? "<0.1 tok/s"
            : "\(tokensPerSecond.formatted(.number.precision(.fractionLength(0...1)))) tok/s"
    }
    var headlineText: String {
        guard tokensPerSecond >= 1 else { return "<1 tok/s" }
        let value = tokensPerSecond.formatted(.number.notation(.compactName)
            .precision(.significantDigits(1...(tokensPerSecond >= 1000 ? 2 : 3))))
        return "\(value) \(tokensPerSecond >= 1000 ? "t/s" : "tok/s")"
    }

    enum Band: Equatable {
        case veryFast, smooth, slow, verySlow

        var label: String {
            switch self {
            case .veryFast: return "Very fast"
            case .smooth: return "Smooth"
            case .slow: return "Slow"
            case .verySlow: return "Very slow"
            }
        }
        var color: Color {
            switch self {
            case .veryFast: return Palette.generationFast
            case .smooth: return Palette.ample
            case .slow: return Palette.watch
            case .verySlow: return Palette.generationSlow
            }
        }
    }

    var band: Band {
        switch tokensPerSecond {
        case ..<10: return .verySlow
        case ..<20: return .slow
        case ..<40: return .smooth
        default:    return .veryFast
        }
    }

    static func parse(_ item: [String: Any], now: Date = Date()) -> LocalModelPerformance? {
        guard item["done"] as? Bool == true, item["error"] == nil,
              let count = positiveInteger(item["eval_count"]),
              let duration = positiveInteger(item["eval_duration"]),
              let tokens = Int(exactly: count) else { return nil }
        return LocalModelPerformance(outputTokens: tokens, durationNanoseconds: duration, measuredAt: now)
    }

    private static func positiveInteger(_ value: Any?) -> Int64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
        // Int64(string) rejects overflow, fractions and non-finite JSON numbers.
        guard let integer = Int64(number.stringValue), integer > 0 else { return nil }
        return integer
    }
}
