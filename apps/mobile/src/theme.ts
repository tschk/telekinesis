/**
 * Shared Telekinesis UI tokens — matches apps/web + apps/desktop tokens.css
 * (feat/apps-web-unpark bf7a87e / desktop e5c31bb: zinc portal + Chivo Mono)
 * and future @tschk/tk-ui. Exact --tk-* names; do not invent alternates. Dark-first.
 */
import type { TextStyle, ViewStyle } from 'react-native';

export const tokens = {
  tkBg: '#09090b',
  tkSurface: '#18181b',
  tkSurface2: '#27272a',
  tkBorder: '#3f3f46',
  tkText: '#d4d4d8',
  tkMuted: '#71717a',
  tkAccent: '#fafafa',
  tkAccentFg: '#09090b',
  tkSuccess: '#34d399',
  tkWarn: '#fbbf24',
  tkDanger: '#f87171',

  tkSpace1: 4,
  tkSpace2: 8,
  tkSpace3: 12,
  tkSpace4: 16,
  tkSpace5: 24,
  tkSpace6: 32,

  tkRadiusSm: 6,
  tkRadiusMd: 10,
  tkRadiusLg: 14,

  /** Both sans + mono = Chivo Mono (portal contract). */
  tkFontSans: 'ChivoMono',
  tkFontMono: 'ChivoMono',
} as const;

export const tokensLight = {
  tkBg: '#fafafa',
  tkSurface: '#ffffff',
  tkSurface2: '#f4f4f5',
  tkBorder: '#d4d4d8',
  tkText: '#18181b',
  tkMuted: '#71717a',
  tkAccent: '#18181b',
  tkAccentFg: '#fafafa',
  tkSuccess: '#059669',
  tkWarn: '#d97706',
  tkDanger: '#dc2626',

  tkSpace1: 4,
  tkSpace2: 8,
  tkSpace3: 12,
  tkSpace4: 16,
  tkSpace5: 24,
  tkSpace6: 32,

  tkRadiusSm: 6,
  tkRadiusMd: 10,
  tkRadiusLg: 14,

  tkFontSans: 'ChivoMono',
  tkFontMono: 'ChivoMono',
} as const;

/** Active theme — dark default (ADE / portal). */
export const colors = tokens;

/** RN approximation of --tk-shadow-sm (dark). */
export const tkShadowSm: ViewStyle = {
  shadowColor: '#000000',
  shadowOffset: { width: 0, height: 2 },
  shadowOpacity: 0.35,
  shadowRadius: 6,
  elevation: 3,
};

export const fontSans: TextStyle = {
  fontFamily: tokens.tkFontSans,
};

export const fontMono: TextStyle = {
  fontFamily: tokens.tkFontMono,
};
