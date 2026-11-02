//! GPIO do SPG200: três portas (A/B/C) de 16 bits, cada uma com buffer de
//! saída, direção, atributo (inversão) e máscara. No V.Smile a porta B traz os
//! botões ON/OFF/restart e a porta C o código de região e os sinais de
//! handshake (CTS/RTS) dos controladores.

use crate::io::SystemIo;

const MODE_WRITE_MASK: u16 = 0x001f;

#[derive(Clone, Copy, Default)]
struct Port {
    buffer: u16,
    dir: u16,
    attrib: u16,
    mask: u16,
}

pub struct Gpio {
    mode: u16,
    ports: [Port; 3],
}

impl Gpio {
    pub fn new() -> Self {
        Self {
            mode: 0,
            ports: [Port::default(); 3],
        }
    }

    pub fn reset(&mut self) {
        self.ports = [Port::default(); 3];
    }

    pub fn mode(&self) -> u16 {
        self.mode
    }
    pub fn set_mode(&mut self, value: u16) {
        self.mode = value & MODE_WRITE_MASK;
    }

    pub fn data<Io: SystemIo>(&self, port: usize, io: &mut Io) -> u16 {
        self.read_gpio(port, io)
    }

    pub fn buffer(&self, port: usize) -> u16 {
        self.ports[port].buffer
    }
    pub fn set_buffer<Io: SystemIo>(&mut self, port: usize, value: u16, io: &mut Io) {
        self.ports[port].buffer = value;
        self.write_gpio(port, io);
    }

    pub fn dir(&self, port: usize) -> u16 {
        self.ports[port].dir
    }
    pub fn set_dir<Io: SystemIo>(&mut self, port: usize, value: u16, io: &mut Io) {
        self.ports[port].dir = value;
        self.write_gpio(port, io);
    }

    pub fn attrib(&self, port: usize) -> u16 {
        self.ports[port].attrib
    }
    pub fn set_attrib<Io: SystemIo>(&mut self, port: usize, value: u16, io: &mut Io) {
        self.ports[port].attrib = value;
        self.write_gpio(port, io);
    }

    pub fn mask(&self, port: usize) -> u16 {
        self.ports[port].mask
    }
    pub fn set_mask<Io: SystemIo>(&mut self, port: usize, value: u16, io: &mut Io) {
        self.ports[port].mask = value;
        self.write_gpio(port, io);
    }

    fn io_out<Io: SystemIo>(port: usize, io: &mut Io) -> u16 {
        match port {
            0 => io.get_port_a(),
            1 => io.get_port_b(),
            2 => io.get_port_c(),
            _ => 0,
        }
    }

    fn read_gpio<Io: SystemIo>(&self, port_index: usize, io: &mut Io) -> u16 {
        let port = &self.ports[port_index];
        let buf = port.buffer ^ (port.dir & !port.attrib);
        let io_out = Self::io_out(port_index, io);
        (buf & (port.dir & !port.mask)) | (io_out & (!port.dir & !port.mask))
    }

    fn write_gpio<Io: SystemIo>(&mut self, port_index: usize, io: &mut Io) {
        let port = &self.ports[port_index];
        let buf = (port.buffer ^ (port.dir & !port.attrib)) & !port.mask;
        let bits = port.dir & !port.mask;
        match port_index {
            0 => io.set_port_a(buf, bits),
            1 => io.set_port_b(buf, bits),
            2 => io.set_port_c(buf, bits),
            _ => {}
        }
    }
}

impl Default for Gpio {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_state!(Port { buffer, dir, attrib, mask });
crate::impl_state!(Gpio { mode, ports });
