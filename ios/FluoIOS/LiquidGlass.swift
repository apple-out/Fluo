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
}

struct LiquidAuroraBackground: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ZStack {
            (colorScheme == .dark
                ? Color(red: 0.03, green: 0.05, blue: 0.09)
                : Color(red: 0.95, green: 0.97, blue: 1.0))
                .ignoresSafeArea()

            Circle()
                .fill(FluoThemeTokens.indigo.opacity(colorScheme == .dark ? 0.26 : 0.18))
                .frame(width: 340, height: 340)
                .blur(radius: 85)
                .offset(x: -110, y: -220)

            Circle()
                .fill(FluoThemeTokens.violet.opacity(colorScheme == .dark ? 0.22 : 0.14))
                .frame(width: 300, height: 300)
                .blur(radius: 90)
                .offset(x: 130, y: -80)

            Circle()
                .fill(FluoThemeTokens.cyan.opacity(colorScheme == .dark ? 0.18 : 0.14))
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

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        content
            .background {
                ZStack {
                    shape.fill(.ultraThinMaterial)
                    if tint != .clear {
                        shape.fill(tint.opacity(0.16))
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

    func body(content: Content) -> some View {
        content
            .background {
                ZStack {
                    Capsule().fill(.ultraThinMaterial)
                    if tint != .clear {
                        Capsule().fill(tint.opacity(0.18))
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

struct LiquidPressButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.95 : 1.0)
            .opacity(configuration.isPressed ? 0.88 : 1.0)
            .animation(.spring(response: 0.24, dampingFraction: 0.72), value: configuration.isPressed)
    }
}

struct LiquidGlassSheetModifier: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 16.4, *) {
            content
                .presentationBackground(.ultraThinMaterial)
                .presentationCornerRadius(32)
        } else {
            content
        }
    }
}

extension View {
    func liquidGlass(cornerRadius: CGFloat = 24, tint: Color = .clear, strokeOpacity: Double = 1.0) -> some View {
        modifier(LiquidGlassSurface(cornerRadius: cornerRadius, tint: tint, strokeOpacity: strokeOpacity))
    }

    func liquidGlassPill(tint: Color = .clear) -> some View {
        modifier(LiquidGlassCapsule(tint: tint))
    }

    func liquidGlassSheet() -> some View {
        modifier(LiquidGlassSheetModifier())
    }
}
