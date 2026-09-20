import Foundation

struct LocalRuntimeReading: Equatable {
    struct Model: Identifiable, Equatable {
        let name: String
        let memoryBytes: Int64?
        let contextLength: Int?
        let quantizationLevel: String?
        let gpuMemoryBytes: Int64?
        let expiresAt: Date?

        init(name: String, memoryBytes: Int64?, contextLength: Int?, quantizationLevel: String?,
             gpuMemoryBytes: Int64? = nil, expiresAt: Date? = nil) {
            self.name = name
            self.memoryBytes = memoryBytes
            self.contextLength = contextLength
            self.quantizationLevel = quantizationLevel
            self.gpuMemoryBytes = gpuMemoryBytes
            self.expiresAt = expiresAt
        }

        var displayedMemoryBytes: Int64? {
            if let gpuMemoryBytes, gpuMemoryBytes > 0 { return gpuMemoryBytes }
            return memoryBytes
        }

        var memoryLabel: String {
            guard let gpuMemoryBytes else { return "Memory" }
            return gpuMemoryBytes > 0 ? "VRAM" : "RAM"
        }

        func unloadText(now: Date) -> String {
            guard let expiresAt else { return "Unavailable" }
            guard expiresAt > now else { return "Pending" }
            let formatter = RelativeDateTimeFormatter()
            formatter.unitsStyle = .abbreviated
            return formatter.localizedString(for: expiresAt, relativeTo: now)
        }

        var id: String { name }
        var brand: LocalModelBrand? { LocalModelBrand.detect(modelName: name) }

        var memoryText: String {
            guard let memoryBytes = displayedMemoryBytes else { return "—" }
            let units: [(String, Double)] = [
                ("EB", pow(1024, 6)), ("PB", pow(1024, 5)),
                ("TB", pow(1024, 4)), ("GB", pow(1024, 3)),
                ("MB", pow(1024, 2)), ("KB", 1024), ("B", 1)
            ]
            let bytes = Double(memoryBytes)
            let (unit, divisor) = units.first { bytes >= $0.1 } ?? ("B", 1)
            let value = (bytes / divisor).formatted(.number.precision(.fractionLength(0...1)))
            return "\(value) \(unit)"
        }

        var contextText: String {
            contextLength.map { "\($0.formatted()) tokens" } ?? "Unavailable"
        }

        var quantizationText: String { quantizationLevel ?? "Unavailable" }

        var detail: String {
            "\(memoryLabel) \(displayedMemoryBytes == nil ? "unavailable" : memoryText) · Context limit \(contextText) · Quantization \(quantizationText)"
        }
    }

    let models: [Model]

    var summary: String {
        models.isEmpty ? "Server reachable · No models loaded"
            : "\(models.count) \(models.count == 1 ? "model" : "models") loaded"
    }
}
