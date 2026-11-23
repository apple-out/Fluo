import SwiftUI

enum FluoThemeTokens {
    static let indigo = Color(red: 0.36, green: 0.55, blue: 1.0)
    static let violet = Color(red: 0.53, green: 0.28, blue: 0.96)
    static let cyan = Color(red: 0.06, green: 0.68, blue: 0.92)
    static let amber = Color(red: 0.96, green: 0.62, blue: 0.16)
    static let coral = Color(red: 0.95, green: 0.36, blue: 0.46)
    static let emerald = Color(red: 0.10, green: 0.76, blue: 0.54)

    static let liquidGradient = LinearGradient(
        colors: [indigo, violet, cyan],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    static let warmEditorialGradient = LinearGradient(
        colors: [indigo, coral, amber],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    static let specularStroke = LinearGradient(
        colors: [
            .white.opacity(0.65),
            .white.opacity(0.18),
            indigo.opacity(0.30),
            .white.opacity(0.45),
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    private static let palette: [Color] = [
        indigo,
        violet,
        cyan,
        emerald,
        amber,
        coral,
    ]

    static func domainAccent(for seed: String) -> Color {
        let trimmed = seed.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !trimmed.isEmpty else { return indigo }
        var hash: UInt64 = 5381
        for scalar in trimmed.unicodeScalars {
            hash = ((hash << 5) &+ hash) &+ UInt64(scalar.value)
        }
        return palette[Int(hash % UInt64(palette.count))]
    }

    static func domainSecondaryAccent(for seed: String) -> Color {
        let trimmed = seed.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !trimmed.isEmpty else { return cyan }
        var hash: UInt64 = 5381
        for scalar in trimmed.unicodeScalars {
            hash = ((hash << 5) &+ hash) &+ UInt64(scalar.value)
        }
        let idx = (Int(hash % UInt64(palette.count)) + 2) % palette.count
        return palette[idx]
    }

    static func domainGradient(for seed: String) -> LinearGradient {
        LinearGradient(
            colors: [domainAccent(for: seed), domainSecondaryAccent(for: seed)],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }

    static var supportsNativeLiquidGlass: Bool {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            return true
        }
        #endif
        return false
    }
}

struct DomainMonogramBadge: View {
    var title: String
    var host: String
    var size: CGFloat = 36
    var cornerRadius: CGFloat = 12
    var symbolFallback: String = "book.closed.fill"

    private var cleanHost: String {
        let raw = host.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if raw.hasPrefix("www.") {
            return String(raw.dropFirst(4))
        }
        return raw
    }

    private var seed: String {
        cleanHost.isEmpty ? title : cleanHost
    }

    private var monogram: String {
        let candidate = cleanHost.isEmpty ? title : cleanHost
        for ch in candidate {
            if ch.isLetter || ch.isNumber {
                return String(ch).uppercased()
            }
        }
        return ""
    }

    var body: some View {
        let accent = FluoThemeTokens.domainAccent(for: seed)
        let secondary = FluoThemeTokens.domainSecondaryAccent(for: seed)
        ZStack {
            RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [accent.opacity(0.92), secondary.opacity(0.72)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                .strokeBorder(.white.opacity(0.38), lineWidth: 0.8)

            if monogram.isEmpty {
                Image(systemName: symbolFallback)
                    .font(.system(size: size * 0.44, weight: .bold))
                    .foregroundStyle(.white)
            } else {
                Text(monogram)
                    .font(.system(size: size * 0.48, weight: .black, design: .rounded))
                    .foregroundStyle(.white)
            }
        }
        .frame(width: size, height: size)
        .shadow(color: accent.opacity(0.28), radius: 6, x: 0, y: 3)
    }
}

struct SpatialIconMedallion: View {
    var icon: String
    var tint: Color
    var size: CGFloat = 34
    var active: Bool = false

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.34, style: .continuous)
                .fill(active ? Color.white.opacity(0.24) : tint.opacity(0.18))
            RoundedRectangle(cornerRadius: size * 0.34, style: .continuous)
                .strokeBorder(active ? Color.white.opacity(0.35) : tint.opacity(0.38), lineWidth: 0.8)
            Image(systemName: icon)
                .font(.system(size: size * 0.45, weight: .bold))
                .foregroundStyle(active ? .white : tint)
        }
        .frame(width: size, height: size)
    }
}

/// Groups multiple Liquid Glass controls so iOS 26+ merges their optical backdrop pass and morphs adjacent shapes.
struct LiquidGlassGroup<Content: View>: View {
    var spacing: CGFloat = 10
    @ViewBuilder var content: () -> Content

    var body: some View {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            GlassEffectContainer(spacing: spacing) {
                content()
            }
        } else {
            content()
        }
        #else
        content()
        #endif
    }
}

struct LiquidAuroraBackground: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let blobScale: Double = FluoThemeTokens.supportsNativeLiquidGlass
            ? 0.14
            : (colorScheme == .dark ? 0.28 : 0.18)
        ZStack {
            if !FluoThemeTokens.supportsNativeLiquidGlass {
                (colorScheme == .dark
                    ? Color(red: 0.035, green: 0.042, blue: 0.075)
                    : Color(red: 0.965, green: 0.972, blue: 0.992))
                    .ignoresSafeArea()
            }

            Circle()
                .fill(FluoThemeTokens.indigo.opacity(blobScale))
                .frame(width: 360, height: 360)
                .blur(radius: 90)
                .offset(x: -120, y: -230)

            Circle()
                .fill(FluoThemeTokens.violet.opacity(blobScale * 0.88))
                .frame(width: 320, height: 320)
                .blur(radius: 95)
                .offset(x: 135, y: -90)

            Circle()
                .fill(FluoThemeTokens.cyan.opacity(blobScale * 0.80))
                .frame(width: 320, height: 320)
                .blur(radius: 95)
                .offset(x: 60, y: 270)

            Circle()
                .fill(FluoThemeTokens.coral.opacity(blobScale * 0.45))
                .frame(width: 240, height: 240)
                .blur(radius: 90)
                .offset(x: -130, y: 190)
        }
        .allowsHitTesting(false)
    }
}

struct LiquidGlassSurface: ViewModifier {
    var cornerRadius: CGFloat = 24
    var tint: Color = .clear
    var strokeOpacity: Double = 1.0
    var interactive: Bool = false
    var prominent: Bool = false

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            let tintAlpha: Double = prominent ? 0.65 : 0.28
            if tint == .clear {
                content.glassEffect(interactive ? .regular.interactive() : .regular, in: shape)
            } else {
                content.glassEffect(
                    interactive ? .regular.tint(tint.opacity(tintAlpha)).interactive() : .regular.tint(tint.opacity(tintAlpha)),
                    in: shape
                )
            }
        } else {
            legacyBody(content: content, shape: shape)
        }
        #else
        legacyBody(content: content, shape: shape)
        #endif
    }

    @ViewBuilder
    private func legacyBody(content: Content, shape: RoundedRectangle) -> some View {
        content
            .background {
                ZStack {
                    if prominent && tint != .clear {
                        shape.fill(FluoThemeTokens.liquidGradient)
                    } else {
                        shape.fill(.ultraThinMaterial)
                        if tint != .clear {
                            shape.fill(tint.opacity(0.16))
                        }
                    }
                    shape.fill(
                        LinearGradient(
                            colors: [.white.opacity(0.20), .white.opacity(0.02), .clear],
                            startPoint: .topLeading,
                            endPoint: .bottom
                        )
                    )
                }
            }
            .overlay(
                shape.strokeBorder(FluoThemeTokens.specularStroke.opacity(strokeOpacity), lineWidth: 1)
            )
            .shadow(color: .black.opacity(0.20), radius: 18, x: 0, y: 8)
    }
}

struct LiquidGlassCapsule: ViewModifier {
    var tint: Color = .clear
    var interactive: Bool = false
    var prominent: Bool = false

    func body(content: Content) -> some View {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            let tintAlpha: Double = prominent ? 0.68 : 0.28
            if tint == .clear {
                content.glassEffect(interactive ? .regular.interactive() : .regular, in: Capsule())
            } else {
                content.glassEffect(
                    interactive ? .regular.tint(tint.opacity(tintAlpha)).interactive() : .regular.tint(tint.opacity(tintAlpha)),
                    in: Capsule()
                )
            }
        } else {
            legacyBody(content: content)
        }
        #else
        legacyBody(content: content)
        #endif
    }

    @ViewBuilder
    private func legacyBody(content: Content) -> some View {
        content
            .background {
                ZStack {
                    if prominent && tint != .clear {
                        Capsule().fill(FluoThemeTokens.liquidGradient)
                    } else {
                        Capsule().fill(.ultraThinMaterial)
                        if tint != .clear {
                            Capsule().fill(tint.opacity(0.18))
                        }
                    }
                    Capsule().fill(
                        LinearGradient(
                            colors: [.white.opacity(0.22), .clear],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )
                }
            }
            .overlay(Capsule().strokeBorder(FluoThemeTokens.specularStroke, lineWidth: 0.9))
            .shadow(color: .black.opacity(0.16), radius: 10, x: 0, y: 4)
    }
}

struct LiquidGlassCircle: ViewModifier {
    var tint: Color = .clear
    var interactive: Bool = true
    var prominent: Bool = false

    func body(content: Content) -> some View {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            let tintAlpha: Double = prominent ? 0.68 : 0.30
            if tint == .clear {
                content.glassEffect(interactive ? .regular.interactive() : .regular, in: Circle())
            } else {
                content.glassEffect(
                    interactive ? .regular.tint(tint.opacity(tintAlpha)).interactive() : .regular.tint(tint.opacity(tintAlpha)),
                    in: Circle()
                )
            }
        } else {
            legacyBody(content: content)
        }
        #else
        legacyBody(content: content)
        #endif
    }

    @ViewBuilder
    private func legacyBody(content: Content) -> some View {
        content
            .background {
                if prominent && tint != .clear {
                    Circle().fill(FluoThemeTokens.liquidGradient)
                } else {
                    Circle().fill(.ultraThinMaterial)
                    if tint != .clear {
                        Circle().fill(tint.opacity(0.20))
                    }
                }
            }
            .overlay(Circle().strokeBorder(.white.opacity(0.34), lineWidth: 0.85))
            .shadow(color: .black.opacity(0.14), radius: 8, x: 0, y: 4)
    }
}

struct LiquidPressButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        if FluoThemeTokens.supportsNativeLiquidGlass {
            configuration.label
        } else {
            configuration.label
                .scaleEffect(configuration.isPressed ? 0.94 : 1.0)
                .opacity(configuration.isPressed ? 0.88 : 1.0)
                .animation(.spring(response: 0.20, dampingFraction: 0.76), value: configuration.isPressed)
        }
    }
}

struct LiquidGlassDockChrome: ViewModifier {
    var tint: Color = .clear

    func body(content: Content) -> some View {
        if FluoThemeTokens.supportsNativeLiquidGlass {
            content
        } else {
            content
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .liquidGlass(cornerRadius: 30, tint: tint)
        }
    }
}

struct LiquidGlassSheetModifier: ViewModifier {
    func body(content: Content) -> some View {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            content
        } else if #available(iOS 16.4, *) {
            content
                .presentationBackground(.ultraThinMaterial)
                .presentationCornerRadius(32)
        } else {
            content
        }
        #else
        if #available(iOS 16.4, *) {
            content
                .presentationBackground(.ultraThinMaterial)
                .presentationCornerRadius(32)
        } else {
            content
        }
        #endif
    }
}

extension View {
    func liquidGlass(
        cornerRadius: CGFloat = 24,
        tint: Color = .clear,
        strokeOpacity: Double = 1.0,
        interactive: Bool = false,
        prominent: Bool = false
    ) -> some View {
        modifier(
            LiquidGlassSurface(
                cornerRadius: cornerRadius,
                tint: tint,
                strokeOpacity: strokeOpacity,
                interactive: interactive,
                prominent: prominent
            )
        )
    }

    func liquidGlassPill(
        tint: Color = .clear,
        interactive: Bool = false,
        prominent: Bool = false
    ) -> some View {
        modifier(LiquidGlassCapsule(tint: tint, interactive: interactive, prominent: prominent))
    }

    func liquidGlassCircle(
        tint: Color = .clear,
        interactive: Bool = true,
        prominent: Bool = false
    ) -> some View {
        modifier(LiquidGlassCircle(tint: tint, interactive: interactive, prominent: prominent))
    }

    func liquidGlassSheet() -> some View {
        modifier(LiquidGlassSheetModifier())
    }

    func liquidGlassDockChrome(tint: Color = .clear) -> some View {
        modifier(LiquidGlassDockChrome(tint: tint))
    }
}
