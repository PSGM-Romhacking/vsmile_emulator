//! Decodificador ADPCM (estilo IMA/DVI) usado pelos canais do SPU.

const STEP_SIZE_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73,
    80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442,
    11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const STEP_ADJUST_TABLE: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

#[derive(Clone, Copy, Default)]
pub struct Adpcm {
    step_index: i32,
    last_sample: i32,
}

impl Adpcm {
    pub fn reset(&mut self) {
        self.step_index = 0;
        self.last_sample = 0;
    }

    /// Decodifica um nibble ADPCM (4 bits) em uma amostra de 16 bits com sinal.
    pub fn decode(&mut self, code: u8) -> i16 {
        let ss = STEP_SIZE_TABLE[self.step_index as usize];
        let mut e = ss / 8
            + if code & 0x1 != 0 { ss / 4 } else { 0 }
            + if code & 0x2 != 0 { ss / 2 } else { 0 }
            + if code & 0x4 != 0 { ss } else { 0 };
        if code & 0x8 != 0 {
            e = -e;
        }
        let sample = (self.last_sample + e).clamp(-32768, 32767);
        self.last_sample = sample;
        self.step_index =
            (self.step_index + STEP_ADJUST_TABLE[(code & 0x07) as usize]).clamp(0, 88);
        sample as i16
    }
}

crate::impl_state!(Adpcm { step_index, last_sample });
