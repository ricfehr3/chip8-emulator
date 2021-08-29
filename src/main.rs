#![feature(exclusive_range_pattern)]

extern crate minifb;

use minifb::{Key, Window, WindowOptions};
use std::{env, io};
use std::io::prelude::*;
use std::io::{stdin, stdout, Read, Write};
use std::fs::File;
use rand::Rng;

// sound
use rodio::{OutputStream, Sink};
use rodio::source::{SineWave, Source};

const PIXEL_WIDTH: usize = 64;
const PIXEL_HEIGHT: usize = 32;
const FOREGROUND_COLOR: u32 = 0xFFFFFFFF;
const BACKGROUND_COLOR: u32 = 0x00000000;
const STACK_SIZE: usize = 16;

const CHIP8_FONTSET: [u8; 80] =
[
    0xF0, 0x90, 0x90, 0x90, 0xF0, //0
    0x20, 0x60, 0x20, 0x20, 0x70, //1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, //2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, //3
    0x90, 0x90, 0xF0, 0x10, 0x10, //4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, //5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, //6
    0xF0, 0x10, 0x20, 0x40, 0x40, //7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, //8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, //9
    0xF0, 0x90, 0xF0, 0x90, 0x90, //A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, //B
    0xF0, 0x80, 0x80, 0x80, 0xF0, //C
    0xE0, 0x90, 0x90, 0x90, 0xE0, //D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, //E
    0xF0, 0x80, 0xF0, 0x80, 0x80  //F
];

fn pause() {
    let mut stdout = stdout();
    stdout.write(b"Press Enter to continue...").unwrap();
    stdout.flush().unwrap();
    stdin().read(&mut [0]).unwrap();
}

struct Chip8 {
    memory: [u8; 0x1000],
    registers: [u8; 16],
    pc: usize,
    old_pc: usize,
    index_register: u16, // actually 12 bits
    delay_timer: u8,
    sound_timer: u8,
    gfx: [u8; PIXEL_WIDTH * PIXEL_HEIGHT],
    draw_flag: bool,
	keys: u32,
    sp: usize,
    stack: [usize; STACK_SIZE],
    sound_iterator: u32,
}

impl Chip8 {
    fn new() -> Chip8 {
        let mut memory_tmp: [u8; 0x1000] = [0; 0x1000];

        for x in 0..80 {
            memory_tmp[x] = CHIP8_FONTSET[x];
        }

        Chip8 {
            memory: memory_tmp,
            registers: [0; 16],
            pc: 0x200,
            old_pc: 0x200,
            index_register: 0x0000,
            delay_timer: 0x0,
            sound_timer: 0x0,
            gfx: [0; PIXEL_WIDTH * PIXEL_HEIGHT],
            draw_flag: false,
			keys: 0x00000000,
            sp: 0,
            stack: [0; STACK_SIZE],
            sound_iterator: 0,
        }
    }

    fn load_rom(&mut self, rom_name: String) -> io::Result<()> {
        let mut f = File::open(rom_name)?;
        f.read(&mut self.memory[0x200 ..]);
        Ok(()) 
    }

    fn fetch_opcode(&mut self) -> io::Result<u16> {
        let first_byte = self.memory[self.pc];
        let second_byte = self.memory[self.pc + 1];
        let opcode = ((first_byte as u16) << 8) | (second_byte as u16);
        self.pc += 2;
        if self.pc > 0xFFE {
            panic!("Program counter is out of memory range");
        }
        Ok(opcode)
    }

    fn op_00E0(&mut self) {
        self.gfx.iter_mut().for_each(|m| *m = 0);
        self.draw_flag = true;
    }

    fn op_00EE(&mut self) {
        self.sp -= 1;
        self.pc = self.stack[self.sp];
    }

    fn op_1nnn(&mut self, nnn: usize) {
        self.pc = nnn; 
    }

    fn op_2nnn(&mut self, nnn: usize) {
        self.stack[self.sp] = self.pc;
        self.sp += 1;
        self.pc = nnn;
    }

    fn op_3xkk(&mut self, x: usize, kk: u8) {
        if self.registers[x] == kk {
            self.pc += 2;
        }
    }

    fn op_4xkk(&mut self, x: usize, kk: u8) {
        if self.registers[x] != kk {
            self.pc += 2;
        }
    }

    fn op_5xy0(&mut self, x: usize, y: usize) {
        if self.registers[x] == self.registers[y] { self.pc += 2 }; 
    }

    fn op_6xkk(&mut self, x: usize, kk: u8) {
        self.registers[x] = kk;
    }

    fn op_7xkk(&mut self, x: usize, kk: u8) {
        self.registers[x] = self.registers[x].wrapping_add(kk);
    }

    fn op_8xy0(&mut self, x: usize, y: usize) {
        self.registers[x] = self.registers[y];
    }

    fn op_8xy1(&mut self, x: usize, y: usize) {
        self.registers[x] = self.registers[x] | self.registers[y]; 
    }

    fn op_8xy2(&mut self, x: usize, y: usize) {
        self.registers[x] = self.registers[x] & self.registers[y]; 
    }

    fn op_8xy3(&mut self, x: usize, y: usize) {
        self.registers[x] = self.registers[x] ^ self.registers[y]; 
    }

    fn op_8xy4(&mut self, x: usize, y: usize) {
        if (self.registers[x] as usize) + (self.registers[y] as usize) > 255 {
            self.registers[0xF] = 0x1;
        } else {
            self.registers[0xF] = 0x0;
        }
        self.registers[x] = self.registers[x].wrapping_add(self.registers[y]);
    }

    fn op_8xy5(&mut self, x: usize, y: usize) {
        if (self.registers[x] as usize) < (self.registers[y] as usize) {
            self.registers[0xF] = 0x0;
        } else {
            self.registers[0xF] = 0x1;
        }
        self.registers[x] = self.registers[x].wrapping_sub(self.registers[y]);
    }

    fn op_8xy6(&mut self, x: usize, y: usize) {
        let lsb = self.registers[x] & 0x1;
        self.registers[0xF] = lsb;
        self.registers[x] = self.registers[x] >> 1;
    }

    fn op_8xy7(&mut self, x: usize, y: usize) {
    }

    fn op_8xyE(&mut self, x: usize, y: usize) {
        let msb = self.registers[x] >> 7 & 0x1;
        self.registers[0xF] = msb;
        self.registers[x] = self.registers[x] << 1;
    }

    fn op_9xy0(&mut self, x: usize, y: usize) {
        if self.registers[x] != self.registers[y] {
            self.pc += 2; 
        }
    }

    fn op_Annn(&mut self, nnn: usize) {
        self.index_register = nnn as u16;
    }

    fn op_Bnnn(&mut self, nnn: usize) {
        let offset = self.registers[0x00] as usize;
        self.pc = nnn + offset;
    }

    fn op_Cxkk(&mut self, x: usize, kk: u8) {
        let rando: u8 = rand::thread_rng().gen();
        self.registers[x] = rando & kk;
    }

    fn op_Dxyn(&mut self, x: usize, y: usize, n: usize) {
        self.draw_flag = true;
        self.registers[0xF] = 0;
        let x_coord = self.registers[x] as usize;
        let y_coord = self.registers[y] as usize;
        let offset = self.index_register as usize;
        for i in 0..n {
            let pixel = self.memory[offset + i];
            for x in 0..8 {
                if pixel & (0x80 >> x) != 0 {
                    let offset = (x_coord + x)+(PIXEL_WIDTH*(y_coord + i));
                    if offset < PIXEL_WIDTH * PIXEL_HEIGHT {
                        if self.gfx[offset] ^ 1 == 0 {
                            self.registers[0xF] = 1;
                        }
                        self.gfx[offset] ^= 1;
                    }
                }
            }
        }
    }

    fn op_Ex9E(&mut self, x: usize) {
        let offset = self.registers[x];
        if (self.keys >> offset) & 0x1 != 0 {
            self.pc += 2; 
        }
    }

    fn op_ExA1(&mut self, x: usize) {
        let offset = self.registers[x];
        if (self.keys >> offset) & 0x1 == 0 {
            self.pc += 2; 
        }
    }

    fn op_Fx07(&mut self, x: usize) {
        self.registers[x] = self.delay_timer;
    }

    fn op_Fx0A(&mut self, x: usize) {
        if self.keys == 0 {
            self.pc -= 2;
        } else {
            for i in 0..0x10 {
                if (self.keys >> i) & 0x1 == 1 {
                    self.registers[x] = i;
                }
            }
        }
    }

    fn op_Fx15(&mut self, x: usize) {
        self.delay_timer = self.registers[x];
    }

    fn op_Fx18(&mut self, x: usize) {
        self.sound_timer = self.registers[x];
    }

    fn op_Fx1E(&mut self, x: usize) {
        self.index_register += self.registers[x] as u16;
    }

    fn op_Fx29(&mut self, x: usize) {
        self.index_register = (self.registers[x] as u16) * 5;
    }

    fn op_Fx33(&mut self, x: usize) {
        let value = self.registers[x];
        self.memory[self.index_register as usize] = (value / 100) % 10;
        self.memory[self.index_register as usize + 1] = (value / 10) % 10;
        self.memory[self.index_register as usize + 2] = (value / 1) % 10;
    }

    fn op_Fx55(&mut self, x: usize) {
        for i in 0..(x + 1) {
            self.memory[(self.index_register as usize) + i] = self.registers[i];
        }
        self.index_register = self.index_register + (x as u16) + 1;
    }

    fn op_Fx65(&mut self, x: usize) {
        for i in 0..(x + 1) {
            self.registers[i] = self.memory[(self.index_register as usize) + i];
        }
        self.index_register = self.index_register + (x as u16) + 1;
    }

    fn execute_opcode(&mut self, opcode: u16) {
        println!("pc: {:#04x}, opcode: {:#04x}", self.pc-2, opcode);
        self.print_registers();
        let x_reg = ((opcode >> 8) & 0x000F) as usize;
        let y_reg = ((opcode >> 4) & 0x000F) as usize;
        let nnn = (opcode & 0x0FFF) as usize;
        let kk = (opcode & 0x00FF) as u8;
        let n = (opcode & 0x000F) as usize;
        let byte_instruction = kk;
        let nibble_instruction = n;

        match opcode {
            0x00E0 => self.op_00E0(),
            0x00EE => self.op_00EE(),
            0x1000..0x1FFF => self.op_1nnn(nnn),
            0x2000..0x2FFF => self.op_2nnn(nnn),
            0x3000..0x3FFF => self.op_3xkk(x_reg, kk),
            0x4000..0x4FFF => self.op_4xkk(x_reg, kk),
            0x5000..0x5FFF => {
                match nibble_instruction {
                    0x0 => self.op_5xy0(x_reg, y_reg),
                    _ => panic!("Invalid opcode {:#04x}", opcode),
                };
            },
            0x6000..0x6FFF => self.op_6xkk(x_reg, kk),
            0x7000..0x7FFF => self.op_7xkk(x_reg, kk),
            0x8000..0x8FFF => {
                match nibble_instruction {
                    0x0 => self.op_8xy0(x_reg, y_reg),
                    0x1 => self.op_8xy1(x_reg, y_reg),
                    0x2 => self.op_8xy2(x_reg, y_reg),
                    0x3 => self.op_8xy3(x_reg, y_reg),
                    0x4 => self.op_8xy4(x_reg, y_reg),
                    0x5 => self.op_8xy5(x_reg, y_reg),
                    0x6 => self.op_8xy6(x_reg, y_reg),
                    0x7 => self.op_8xy7(x_reg, y_reg),
                    0xE => self.op_8xyE(x_reg, y_reg),
                    _ => panic!("Invalid opcode {:#04x}", opcode),
                };
            },
            0x9000..0x9FFF => {
                match nibble_instruction {
                    0x0 => self.op_9xy0(x_reg, y_reg),
                    _ => panic!("Invalid opcode {:#04x}", opcode),
                };
            },
            0xA000..0xAFFF => self.op_Annn(nnn),
            0xB000..0xBFFF => self.op_Bnnn(nnn),
            0xC000..0xCFFF => self.op_Cxkk(x_reg, kk),
            0xD000..0xDFFF => self.op_Dxyn(x_reg, y_reg, n),
            0xE000..0xEFFF => {
                match byte_instruction {
                    0x9E => self.op_Ex9E(x_reg),
                    0xA1 => self.op_ExA1(x_reg),
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                }
            },
            0xF000..0xFFFF => {
                match byte_instruction {
                    0x07 => self.op_Fx07(x_reg),
                    0x0A => self.op_Fx0A(x_reg),
                    0x15 => self.op_Fx15(x_reg),
                    0x18 => self.op_Fx18(x_reg),
                    0x1E => self.op_Fx1E(x_reg),
                    0x29 => self.op_Fx29(x_reg),
                    0x33 => self.op_Fx33(x_reg),
                    0x55 => self.op_Fx55(x_reg),
                    0x65 => self.op_Fx65(x_reg),
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                }
            }
            
            _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
        }

    }

    fn step(&mut self) {
        let opcode = self.fetch_opcode().unwrap();

        if self.sound_iterator % 16 == 0 {
            if self.sound_timer > 0 {
                self.sound_timer -= 1;
            }

            if self.delay_timer > 0 {
                self.delay_timer -= 1;
            }

            self.sound_iterator = 0;
        }

        self.sound_iterator = self.sound_iterator.wrapping_add(1);

        self.execute_opcode(opcode);
    }

	fn set_key(&mut self, key: u8) {
		//self.keys |= (0x1 << (key - 1));	
		self.keys |= 0x1 << key;	
	}

	fn clear_keys(&mut self) {
        self.keys = 0x0000;
	}	

    fn print_memory(&self) {
        let mut x: u32 = 0;
        for byte in &self.memory {
            if x % 0x10 == 0 {
                println!("");
                print!("{:#03x}: ", x);
            }
            print!("{:#03x} ", byte);
            x += 1;
        }
        println!("");
    }

    fn print_registers(&self) {
        let mut x: u32 = 0;
        for register in &self.registers {
            println!("{:#02x}: {:#02x}", x, register);
            x += 1;
        }
        println!("I: {:#02x}", self.index_register);
        println!("Delay Timer: {:#02x}", self.delay_timer);
        println!("Sound Timer: {:#02x}", self.sound_timer);
    }
}

fn update_graphics(chip8: &mut Chip8, display_buf: &mut Vec<u32>) {
    let mut iter = 0; 
    for pixel in chip8.gfx.iter().cloned() {
        if pixel > 0 {
            display_buf[iter] =  FOREGROUND_COLOR;
        } else {
            display_buf[iter] =  BACKGROUND_COLOR;
        }
        iter += 1;
    }
}

fn main() {
    let rom_name = env::args().nth(1).expect("Missing argument");
    let mut chip8 = Chip8::new();
    let mut display_buf: Vec<u32> = vec![0; PIXEL_WIDTH * PIXEL_HEIGHT];
    let mut options = WindowOptions::default();
    options.scale = minifb::Scale::X16;
    let mut window = Window::new(
        "Chip8 emulator by Ric Fehr",
        PIXEL_WIDTH,
        PIXEL_HEIGHT,
        options,
        )
        .unwrap_or_else(|e| {
            panic!("{}", e);
        });

    // sound
	let (_stream, stream_handle) = OutputStream::try_default().unwrap();
	let sink = Sink::try_new(&stream_handle).unwrap();

	let source = SineWave::new(440).amplify(0.20);
	sink.append(source);

    window.limit_update_rate(Some(std::time::Duration::from_millis(1)));

    chip8.load_rom(rom_name).unwrap();

    chip8.print_memory();

    while window.is_open() && !window.is_key_down(Key::Escape) {
		window.get_keys().map(|keys| {
			for t in keys {
				match t {
					Key::Key1 => chip8.set_key(0x1),
					Key::Key2 => chip8.set_key(0x2),
					Key::Key3 => chip8.set_key(0x3),
					Key::Key4 => chip8.set_key(0xC),
					Key::Q => chip8.set_key(0x4),
					Key::W => chip8.set_key(0x5),
					Key::E => chip8.set_key(0x6),
					Key::R => chip8.set_key(0xD),
					Key::A => chip8.set_key(0x7),
					Key::S => chip8.set_key(0x8),
					Key::D => chip8.set_key(0x9),
					Key::F => chip8.set_key(0xE),
					Key::Z => chip8.set_key(0xA),
					Key::X => chip8.set_key(0x0),
					Key::C => chip8.set_key(0xB),
					Key::V => chip8.set_key(0xF),
					_ => (),
				}
			}
		});

        chip8.step();
        if chip8.draw_flag {
            chip8.draw_flag = false;
            update_graphics(&mut chip8, &mut display_buf);
        }

        if chip8.sound_timer > 0 { sink.play() } else { sink.pause() }

        window
            .update_with_buffer(&display_buf, PIXEL_WIDTH, PIXEL_HEIGHT)
            .unwrap();

        chip8.clear_keys();
    }
}
