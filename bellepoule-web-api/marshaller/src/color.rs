//! Couleurs (remplace `GdkColor` : `gdk_color_parse` / `gdk_color_to_string`).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u16,
    pub g: u16,
    pub b: u16,
}

impl Rgb {
    pub const fn new8(r: u8, g: u8, b: u8) -> Self {
        Rgb { r: r as u16 * 257, g: g as u16 * 257, b: b as u16 * 257 }
    }

    /// `gdk_color_parse` : `#rgb`, `#rrggbb`, `#rrrgggbbb`, `#rrrrggggbbbb`
    /// ou un nom de couleur X11 courant.
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            let n = hex.len();
            if n == 0 || n % 3 != 0 || n > 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let w = n / 3;
            let comp = |i: usize| -> u16 {
                let v = u32::from_str_radix(&hex[i * w..(i + 1) * w], 16).unwrap_or(0);
                // Mise à l'échelle sur 16 bits (comme pango_color_parse).
                let bits = 4 * w as u32;
                let mut out = v << (16 - bits);
                let mut b = bits;
                while b < 16 {
                    out |= out >> b;
                    b *= 2;
                }
                out as u16
            };
            return Some(Rgb { r: comp(0), g: comp(1), b: comp(2) });
        }
        let named = match s.to_ascii_lowercase().replace(' ', "").as_str() {
            "black" => (0, 0, 0),
            "white" => (255, 255, 255),
            "grey" | "gray" => (190, 190, 190),
            "lightgrey" | "lightgray" => (211, 211, 211),
            "darkgrey" | "darkgray" => (169, 169, 169),
            "red" => (255, 0, 0),
            "green" => (0, 255, 0),
            "darkgreen" => (0, 100, 0),
            "blue" => (0, 0, 255),
            "yellow" => (255, 255, 0),
            "orange" => (255, 165, 0),
            "purple" => (160, 32, 240),
            "pink" => (255, 192, 203),
            "brown" => (165, 42, 42),
            "cyan" => (0, 255, 255),
            "magenta" => (255, 0, 255),
            _ => return None,
        };
        Some(Rgb::new8(named.0, named.1, named.2))
    }

    /// `gdk_color_to_string` : `#rrrrggggbbbb`.
    pub fn to_gdk_string(self) -> String {
        format!("#{:04x}{:04x}{:04x}", self.r, self.g, self.b)
    }

    /// `#rrggbb` (CSS).
    pub fn to_css(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r >> 8, self.g >> 8, self.b >> 8)
    }

    pub fn red_f(self) -> f64 {
        self.r as f64 / 65535.0
    }
    pub fn green_f(self) -> f64 {
        self.g as f64 / 65535.0
    }
    pub fn blue_f(self) -> f64 {
        self.b as f64 / 65535.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_formats() {
        assert_eq!(Rgb::parse("#ff0000").unwrap(), Rgb::new8(255, 0, 0));
        assert_eq!(Rgb::parse("#ffff00000000").unwrap(), Rgb::new8(255, 0, 0));
        assert_eq!(Rgb::parse("#f00").unwrap(), Rgb::new8(255, 0, 0));
        assert_eq!(Rgb::parse("lightgrey").unwrap(), Rgb::new8(211, 211, 211));
        let c = Rgb::parse("#fff36c").unwrap();
        assert_eq!(c.to_gdk_string(), "#fffff3f36c6c");
        assert_eq!(Rgb::parse(&c.to_gdk_string()).unwrap(), c);
        assert!(Rgb::parse("nope").is_none());
    }
}
