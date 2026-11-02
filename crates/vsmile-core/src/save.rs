//! Serialização de *save states* — em Rust puro, sem dependências.
//!
//! O console não tem referências cíclicas (`Rc`/`RefCell`) nem `unsafe`, então
//! todo o seu estado é uma árvore de campos `Copy`/`Vec`/`Box` — perfeitamente
//! serializável. Este módulo define um [`State`] mínimo (dump/restore binário
//! little-endian) e o macro [`impl_state!`] que gera a implementação a partir da
//! lista de campos de cada struct, evitando dessincronização entre escrita e
//! leitura.
//!
//! O que **não** entra no save: ROM do sistema e do cartucho (recarregadas do
//! arquivo), temporização de vídeo e campos de diagnóstico. O estado é carregado
//! sobre um console já construído com a mesma configuração.

/// Cursor de leitura sobre os bytes do save.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Consome `n` bytes, devolvendo-os (ou erro se acabou).
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos.checked_add(n).ok_or("save: overflow de leitura")?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or("save: fim inesperado do save state")?;
        self.pos = end;
        Ok(slice)
    }

    /// Verifica que todo o buffer foi consumido (nenhum lixo sobrando).
    pub fn expect_end(&self) -> Result<(), String> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(format!(
                "save: {} bytes não consumidos (save state corrompido?)",
                self.data.len() - self.pos
            ))
        }
    }
}

/// Um valor que sabe se serializar e restaurar. A restauração é *in place*
/// (sobre um valor já alocado), então vetores/arrays mantêm seu tamanho.
pub trait State {
    fn save(&self, w: &mut Vec<u8>);
    fn load(&mut self, r: &mut Reader) -> Result<(), String>;
}

macro_rules! impl_state_primitive {
    ($t:ty) => {
        impl State for $t {
            fn save(&self, w: &mut Vec<u8>) {
                w.extend_from_slice(&self.to_le_bytes());
            }
            fn load(&mut self, r: &mut Reader) -> Result<(), String> {
                const N: usize = core::mem::size_of::<$t>();
                let b = r.take(N)?;
                let mut arr = [0u8; N];
                arr.copy_from_slice(b);
                *self = <$t>::from_le_bytes(arr);
                Ok(())
            }
        }
    };
}

impl_state_primitive!(u8);
impl_state_primitive!(u16);
impl_state_primitive!(u32);
impl_state_primitive!(u64);
impl_state_primitive!(i8);
impl_state_primitive!(i16);
impl_state_primitive!(i32);
impl_state_primitive!(i64);

impl State for bool {
    fn save(&self, w: &mut Vec<u8>) {
        w.push(*self as u8);
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        *self = r.take(1)?[0] != 0;
        Ok(())
    }
}

impl State for usize {
    fn save(&self, w: &mut Vec<u8>) {
        (*self as u64).save(w);
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        let mut v = 0u64;
        v.load(r)?;
        *self = v as usize;
        Ok(())
    }
}

impl<const N: usize, T: State> State for [T; N] {
    fn save(&self, w: &mut Vec<u8>) {
        for x in self.iter() {
            x.save(w);
        }
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        for x in self.iter_mut() {
            x.load(r)?;
        }
        Ok(())
    }
}

impl<T: State> State for [T] {
    fn save(&self, w: &mut Vec<u8>) {
        for x in self.iter() {
            x.save(w);
        }
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        for x in self.iter_mut() {
            x.load(r)?;
        }
        Ok(())
    }
}

// `Box<[u16;N]>` (ex.: a RAM): delega ao array interno.
impl<T: State> State for Box<T> {
    fn save(&self, w: &mut Vec<u8>) {
        (**self).save(w);
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        (**self).load(r)
    }
}

// `Box<[T]>` (ex.: framebuffer, buffer de áudio): tamanho fixo por construção,
// restaurado in place sem prefixo de comprimento.
impl<T: State> State for Box<[T]> {
    fn save(&self, w: &mut Vec<u8>) {
        for x in self.iter() {
            x.save(w);
        }
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        for x in self.iter_mut() {
            x.load(r)?;
        }
        Ok(())
    }
}

// `VecDeque<u8>` (fila de saída da UART do joystick): com prefixo de tamanho.
impl State for std::collections::VecDeque<u8> {
    fn save(&self, w: &mut Vec<u8>) {
        (self.len() as u64).save(w);
        for &b in self.iter() {
            w.push(b);
        }
    }
    fn load(&mut self, r: &mut Reader) -> Result<(), String> {
        let mut n = 0u64;
        n.load(r)?;
        self.clear();
        for _ in 0..n {
            self.push_back(r.take(1)?[0]);
        }
        Ok(())
    }
}

/// Gera `impl State` para uma struct a partir da lista de campos a serializar.
/// Os campos ausentes da lista (config, diagnóstico) são ignorados.
#[macro_export]
macro_rules! impl_state {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl $crate::save::State for $t {
            fn save(&self, w: &mut Vec<u8>) {
                $( $crate::save::State::save(&self.$f, w); )*
            }
            fn load(&mut self, r: &mut $crate::save::Reader) -> Result<(), String> {
                $( $crate::save::State::load(&mut self.$f, r)?; )*
                Ok(())
            }
        }
    };
}
