//! Configurações de visualização da PPU (úteis para debug: ligar/desligar
//! planos de fundo, sprites ou camadas específicas).

/// Quais camadas/planos a PPU deve desenhar.
#[derive(Clone, Copy, Debug)]
pub struct PpuViewSettings {
    /// Mostrar sprites em cada uma das 4 camadas de profundidade.
    pub show_sprites_in_layer: [bool; 4],
    /// Mostrar cada um dos 2 planos de fundo.
    pub show_bg: [bool; 2],
    /// Mostrar sprites (chave geral).
    pub show_sprites: bool,
}

impl Default for PpuViewSettings {
    fn default() -> Self {
        Self {
            show_sprites_in_layer: [true; 4],
            show_bg: [true; 2],
            show_sprites: true,
        }
    }
}
