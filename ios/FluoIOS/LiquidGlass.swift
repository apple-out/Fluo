import SwiftUI

enum FluoThemeTokens {
    static let indigo = Color(red: 0.36, green: 0.55, blue: 1.0)
    static let violet = Color(red: 0.49, green: 0.23, blue: 0.93)
    static let cyan = Color(red: 0.05, green: 0.65, blue: 0.91)

    static let liquidGradient = LinearGradient(
        colors: [indigo, violet, cyan],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    static let specularStroke = LinearGradient(
        colors: [
            .white.opacity(0.62),
            .white.opacity(0.18),
            indigo.opacity(0.28),
            .white.opacity(0.42),
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    static var supportsNativeLiquidGlass: Bool {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            return true
        }
        #endif
        return false
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
        let blobScale: Double = FluoThemeTokens.supportsNativeLiquidGlass ? 0.12 : (colorScheme == .dark ? 0.24 : 0.16)
        ZStack {
            if !FluoThemeTokens.supportsNativeLiquidGlass {
                (colorScheme == .dark
                    ? Color(red: 0.03, green: 0.05, blue: 0.09)
                    : Color(red: 0.95, green: 0.97, blue: 1.0))
                    .ignoresSafeArea()
            }

            Circle()
                .fill(FluoThemeTokens.indigo.opacity(blobScale))
                .frame(width: 340, height: 340)
                .blur(radius: 85)
                .offset(x: -110, y: -220)

            Circle()
                .fill(FluoThemeTokens.violet.opacity(blobScale * 0.85))
                .frame(width: 300, height: 300)
                .blur(radius: 90)
                .offset(x: 130, y: -80)

            Circle()
                .fill(FluoThemeTokens.cyan.opacity(blobScale * 0.75))
                .frame(width: 320, height: 320)
                .blur(radius: 95)
                .offset(x: 40, y: 260)
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
            .shadow(color: .black.opacity(0.22), radius: 18, x: 0, y: 8)
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
                    Circle().fill(.white.opacity(tint == .clear ? 0.10 : 0.18))
                }
            }
            .overlay(Circle().strokeBorder(.white.opacity(0.32), lineWidth: 0.8))
    }
}

struct LiquidPressButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        if FluoThemeTokens.supportsNativeLiquidGlass {
            configuration.label
        } else {
            configuration.label
                .scaleEffect(configuration.isPressed ? 0.95 : 1.0)
                .opacity(configuration.isPressed ? 0.88 : 1.0)
                .animation(.spring(response: 0.24, dampingFraction: 0.72), value: configuration.isPressed)
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
            // iOS 26 sheets use native system Liquid Glass chrome; overriding presentationBackground disables it.
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
