//! Divisores de clock usados pelos periféricos.
//!
//! O SPG200 roda a 27 MHz. Vários periféricos derivam frequências menores
//! (32768 Hz, 4096 Hz, …) por contadores fracionários: a cada `cycles` ciclos
//! de CPU subtrai-se `b*cycles` de um acumulador; quando ele cruza zero, soma-se
//! `a` e dispara-se um "tick" da frequência derivada.

/// Divisor de clock com contador de divisões (para derivar sub-frequências por
/// potências de dois via [`DivisibleClock::divided_tick`]).
pub struct DivisibleClock {
    counter: i64,
    div_counter: u64,
    a: i64,
    b: i64,
}

impl DivisibleClock {
    pub fn new(a: i64, b: i64) -> Self {
        Self {
            counter: a,
            div_counter: 0,
            a,
            b,
        }
    }

    #[inline]
    pub fn tick(&mut self, cycles: u32) -> bool {
        self.counter -= self.b * cycles as i64;
        if self.counter <= 0 {
            self.counter += self.a;
            self.div_counter = self.div_counter.wrapping_add(1);
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self) {
        self.counter = self.a;
        self.div_counter = 0;
    }

    /// `true` quando o contador de divisões é múltiplo de `2^div` — ou seja, na
    /// borda da frequência base dividida por `2^div`.
    #[inline]
    pub fn divided_tick(&self, div: u32) -> bool {
        self.div_counter & ((1u64 << div) - 1) == 0
    }

    pub fn clear_div_counter(&mut self) {
        self.div_counter = 0;
    }
}

/// Clock simples (down-counter) sem divisor.
pub struct SimpleClock {
    counter: i64,
    a: i64,
}

impl SimpleClock {
    pub fn new(a: i64) -> Self {
        Self { counter: a, a }
    }

    #[inline]
    pub fn tick(&mut self, cycles: u32) -> bool {
        self.counter -= cycles as i64;
        if self.counter <= 0 {
            self.counter += self.a;
            true
        } else {
            false
        }
    }

    pub fn reset(&mut self) {
        self.counter = self.a;
    }
}

crate::impl_state!(SimpleClock { counter, a });
crate::impl_state!(DivisibleClock { counter, div_counter, a, b });
