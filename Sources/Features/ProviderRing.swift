import SwiftUI

/// The ring around a provider glyph: a grey track with a coloured arc that
/// starts at 12 o'clock and sweeps clockwise by the fraction used.
///
/// When that provider is doing something right now, a second, much thinner arc
/// appears *inside* the ring, in the gap between the glyph and the track. It is
/// deliberately a different radius, a different weight and a neutral colour, so
/// it reads as a separate fact rather than as the usage number moving.
struct ProviderRing: View {
    /// Nil when the provider reports what is left but never says out of what —
    /// there is no arc to draw, and inventing one would be a lie in a shape.
    let usedFraction: Double?
    let glyph: ProviderGlyph
    var isStale: Bool = false
    /// Blocked right now. Shown as spent whatever the arc says, because that is
    /// what it means for you — a ring reading 16% while the account is paused
    /// is technically true and practically a lie.
    var isBlocked: Bool = false
    var activity: ActivitySummary?
    /// A fetch this cell asked for, in flight.
    var isRefreshing: Bool = false
    var localPerformance: LocalModelPerformance?
    /// The weekly limit, when the provider has one. Nil is the ordinary case
    /// for a provider with a single window, and draws nothing.
    var weeklyFraction: Double?
    /// Where the user asked for it, if at all.
    var weeklyRing: WeeklyRing = .off

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.codenotchReduceTransparency) private var reduceTransparency
    @Environment(\.codenotchAccentColor) private var accentColor
    @State private var spin: Double = 0

    private var band: UsageBand {
        isBlocked ? .exhausted : UsageBand.band(for: usedFraction ?? 0)
    }
    private var sweep: CGFloat { CGFloat(min(max(usedFraction ?? 0, 0), 1)) }

    private var weeklyBand: UsageBand {
        isBlocked ? .exhausted : UsageBand.band(for: weeklyFraction ?? 0)
    }
    private var weeklySweep: CGFloat { CGFloat(min(max(weeklyFraction ?? 0, 0), 1)) }

    /// Inside, the weekly ring and the working indicator want the same band —
    /// 1.03pt apart, one of them spinning. Rather than shave both until neither
    /// is legible, the transient one wins: while a provider is working that is
    /// the more urgent fact, and the week is still a hover away. Outside there
    /// is no contest, so nothing is given up there.
    private var isWorking: Bool {
        weeklyRing == .inside && activity != nil && activity?.state != .idle
    }

    var body: some View {
        ZStack {
            // Dimming applies to the usage reading only. Whether Claude is
            // working right now is known first-hand and stays at full strength
            // even when the percentage behind it has gone stale.
            ZStack {
                Circle()
                    .strokeBorder(Palette.ringTrack, lineWidth: NotchLayout.trackStroke)

                if let localPerformance {
                    Circle()
                        .strokeBorder(localPerformance.band.color, lineWidth: NotchLayout.progressStroke)
                        .animation(NotchMotion.reading, value: localPerformance.band)
                } else if usedFraction != nil {
                    Circle()
                        .inset(by: NotchLayout.trackStroke / 2)
                        .trim(from: 0, to: sweep)
                        .stroke(
                            band.color(accent: accentColor),
                            style: StrokeStyle(lineWidth: NotchLayout.progressStroke, lineCap: .round)
                        )
                        // Refreshing spins the reading itself rather than
                        // overlaying a separate spinner: the thing being
                        // refetched is the thing that should move, and a second
                        // arc on the same track only competes with it.
                        .rotationEffect(.degrees(-90 + spin))
                        // A ring that snaps to a new value reads as a glitch; one
                        // that sweeps reads as a measurement being taken.
                        .animation(NotchMotion.reading, value: sweep)
                        .animation(NotchMotion.reading, value: band)
                }

                // The weekly limit, when there is one and it has been asked
                // for. Same start and direction as the headline arc, so the
                // two are read the same way round; thinner and at its own
                // radius, so which is which never has to be worked out.
                //
                // It carries its own band colour rather than borrowing the
                // headline's: a session at 12% beside a week at 91% is exactly
                // the case this exists for, and painting them the same colour
                // would hide it. Held slightly back in opacity so the headline
                // stays the one the eye lands on first.
                if let radius = weeklyRing.radius, weeklyFraction != nil, !isWorking {
                    let inset = NotchLayout.ringDiameter / 2 - radius

                    // A track of its own, for the same reason the headline has
                    // one: a week nobody has spent yet draws an arc of zero
                    // length, and without something behind it that is
                    // indistinguishable from the feature being broken. Codex
                    // opened its week at 0% and read as missing.
                    Circle()
                        .inset(by: inset)
                        .stroke(Palette.ringTrack,
                                style: StrokeStyle(lineWidth: NotchLayout.weeklyRingStroke))
                        .opacity(reduceTransparency ? 1 : 0.7)

                    Circle()
                        .inset(by: inset)
                        .trim(from: 0, to: weeklySweep)
                        .stroke(
                            weeklyBand.color(accent: accentColor),
                            style: StrokeStyle(lineWidth: NotchLayout.weeklyRingStroke,
                                               lineCap: .round)
                        )
                        .opacity(reduceTransparency ? 1 : 0.8)
                        .rotationEffect(.degrees(-90))
                        .animation(NotchMotion.reading, value: weeklySweep)
                        .animation(NotchMotion.reading, value: weeklyBand)
                }

                ProviderGlyphView(glyph: glyph)
                    .foregroundStyle(Palette.textPrimary)
                    // A spent limit dims its glyph so the ring reads as "waiting".
                    // Under reduce-transparency, boost opacity so it stays legible without low alpha.
                    .opacity(band == .exhausted ? (reduceTransparency ? 0.7 : 0.35) : 1)
            }
            .opacity(isStale ? (reduceTransparency ? 0.75 : 0.45) : 1)

            if let activity, activity.state != .idle {
                ActivityArc(summary: activity)
            }
        }
        .frame(width: NotchLayout.ringDiameter, height: NotchLayout.ringDiameter)
        // Pressed in while it works, and released when the answer lands. The
        // ring is the button, so the ring is what should feel pressed.
        .scaleEffect(isRefreshing ? 0.93 : 1)
        .animation(.spring(response: 0.3, dampingFraction: 0.62), value: isRefreshing)
        .onChange(of: isRefreshing) { _, refreshing in
            guard refreshing, !reduceMotion else { return }
            // Exactly one turn, and it stops by itself.
            //
            // The obvious spelling is a `repeatForever` linear spin started on
            // the way in and cancelled on the way out — but `repeatForever` does
            // not stop when you set the value back, and if the value you set is
            // the one it is already animating toward, nothing changes and it
            // simply keeps going. The ring then spins for ever after a refresh
            // that finished half a second in.
            //
            // A single finite turn has no cancellation problem at all: 360° is
            // the same angle as 0°, so it lands exactly where the reading
            // belongs. It eases out, so it settles rather than stopping dead.
            withAnimation(.timingCurve(0.32, 0, 0.14, 1, duration: 0.95)) {
                spin += 360
            }
        }
    }
}

/// The inner indicator: a short arc that spins while work is happening, and a
/// full pulsing ring when something is blocked waiting on you.
private struct ActivityArc: View {
    let summary: ActivitySummary

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.codenotchReduceTransparency) private var reduceTransparency
    @State private var spinning = false
    @State private var pulsing = false

    /// How much of the circle the moving arc covers.
    private let arcFraction: CGFloat = 0.25

    private var inset: CGFloat {
        (NotchLayout.ringDiameter - NotchLayout.activityDiameter) / 2
    }

    var body: some View {
        Group {
            switch summary.state {
            case .working: spinner
            case .waiting, .success: pulse
            case .idle:    EmptyView()
            }
        }
        .frame(width: NotchLayout.ringDiameter, height: NotchLayout.ringDiameter)
    }

    private var spinner: some View {
        Circle()
            .inset(by: inset)
            .trim(from: 0, to: arcFraction)
            .stroke(
                summary.color,
                style: StrokeStyle(lineWidth: NotchLayout.activityStroke, lineCap: .round)
            )
            .rotationEffect(.degrees(spinning ? 360 : 0))
            .onAppear {
                guard !reduceMotion else { return }
                withAnimation(.linear(duration: 1.1).repeatForever(autoreverses: false)) {
                    spinning = true
                }
            }
            .onDisappear { spinning = false }
    }

    private var pulse: some View {
        Circle()
            .inset(by: inset)
            .stroke(summary.color, lineWidth: NotchLayout.activityStroke)
            .opacity(pulsing ? (reduceTransparency ? 0.65 : 0.3) : 1)
            .onAppear {
                guard !reduceMotion else { return }
                withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) {
                    pulsing = true
                }
            }
            .onDisappear { pulsing = false }
    }
}

struct ProviderCell: View {
    let snapshot: ProviderSnapshot
    var activity: ActivitySummary?
    var isRefreshing: Bool = false
    var weeklyRing: WeeklyRing = .off

    /// A dash, not "0%": nothing read is not the same as nothing used.
    private var readingText: String {
        snapshot.hasReading ? snapshot.headlineText : "—"
    }

    var body: some View {
        VStack(spacing: NotchLayout.ringLabelGap) {
            ProviderRing(
                usedFraction: snapshot.hasReading ? snapshot.ringFraction : nil,
                glyph: snapshot.glyph,
                isStale: snapshot.status.isStale || !snapshot.hasReading,
                isBlocked: snapshot.block != nil,
                activity: activity,
                isRefreshing: isRefreshing,
                localPerformance: snapshot.localPerformance,
                weeklyFraction: snapshot.hasReading ? snapshot.weeklyFraction : nil,
                weeklyRing: weeklyRing
            )
            Text(readingText)
                .font(Typography.percent)
                .foregroundStyle(snapshot.showsLocalPerformance && snapshot.localPerformance == nil
                                 ? Palette.textSecondary : Palette.textPrimary)
                // Keep local speeds inside the ring's column so longer units
                // cannot consume the notch's existing side margins.
                .lineLimit(1)
                .minimumScaleFactor(snapshot.localModel == nil ? 1 : 0.5)
                .fixedSize(horizontal: snapshot.localModel == nil, vertical: false)
                .frame(width: snapshot.localModel == nil ? nil : NotchLayout.ringDiameter,
                       height: NotchLayout.percentLineHeight)
                .contentTransition(.numericText())
                .animation(NotchMotion.reading, value: readingText)
        }
        .frame(height: NotchLayout.cellExtent)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(snapshot.localModel.map {
            "\($0.brand.map { "\($0.displayName), " } ?? "")\($0.name), \(snapshot.displayName) local, \(snapshot.showsLocalPerformance ? (snapshot.localPerformance.map { "Last generation speed \($0.speedText), \($0.band.label)" } ?? "Speed not measured") : "Loaded"), \($0.detail)\(activity?.state == .working ? ", Thinking" : "")"
        } ?? "\(snapshot.displayName), \(readingText)")
    }
}
