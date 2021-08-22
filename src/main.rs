#![feature(exclusive_range_pattern)]

extern crate minifb;

use minifb::{Key, Window, WindowOptions};
use std::{env, io};
use std::io::prelude::*;
use std::io::{stdin, stdout, Read, Write};
use std::fs::File;
use rand::Rng;

const PIXEL_WIDTH: usize = 64;
const PIXEL_HEIGHT: usize = 32;
const FOREGROUND_COLOR: u32 = 0xFFFFFFFF;
const BACKGROUND_COLOR: u32 = 0x00000000;

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
	keys: u16,
}

impl Chip8 {
    fn new() -> Chip8 {
        Chip8 {
            memory: [0; 0x1000],
            registers: [0; 16],
            pc: 0x200,
            old_pc: 0x200,
            index_register: 0x0000,
            delay_timer: 0x0,
            sound_timer: 0x0,
            gfx: [0; PIXEL_WIDTH * PIXEL_HEIGHT],
            draw_flag: false,
			keys: 0x0000,
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

    fn set_register(&mut self, offset: u8, value: u8) {
        self.registers[offset as usize] = value;
    }

    fn step(&mut self) {
        let opcode = self.fetch_opcode().unwrap();
        //println!("pc: {:#04x}, opcode: {:#04x}", self.pc, opcode);
        match opcode {
            0x00E0 => {
                self.gfx.iter_mut().for_each(|m| *m = 0);
            }, 
            0x00EE => {
                self.pc = self.old_pc;
            },
            0x1000..0x1FFF => {
                self.pc = (opcode &0xFFF) as usize; 
            },
            0x2000..0x2FFF => {
                self.old_pc = self.pc;
                self.pc = (opcode & 0xFFF) as usize;
            },
            0x3000..0x3FFF => {
                let register_offset = ((opcode >> 8) & 0xF) as usize;
                let value = (opcode & 0xFF) as u8;
                if self.registers[register_offset] == value {
                    self.pc += 2;
                }
            },
            0x4000..0x4FFF => {
                let register_offset = ((opcode >> 8) & 0xF) as usize;
                let value = (opcode & 0xFF) as u8;
                if self.registers[register_offset] != value {
                    self.pc += 2;
                }
            },
            0x5000..0x5FFF => {
                let x_reg = ((opcode >> 8) & 0xF) as usize;
                let y_reg = ((opcode >> 4) & 0xF) as usize;
                let instruction = (opcode & 0xF) as usize;
                match instruction {
                    0x0 => if self.registers[x_reg] == self.registers[y_reg] {
                        self.pc += 2; 
                    },
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                };
            },
            0x6000..0x6FFF => {
                let register_offset = ((opcode >> 8) & 0xF) as u8;
                let value = (opcode & 0xFF) as u8;
                self.set_register(register_offset, value);
            },
            0x7000..0x7FFF => {
                let register_offset = ((opcode >> 8) & 0xF) as usize;
                let value = (opcode & 0xFF) as u8;
                self.registers[register_offset] = self.registers[register_offset].wrapping_add(value);
            },
            0x8000..0x8FFF => {
                let x_reg = ((opcode >> 8) & 0xF) as usize;
                let y_reg = ((opcode >> 4) & 0xF) as usize;
                let instruction = (opcode & 0xF) as usize;
                match instruction {
                    0x0 => {
                        self.registers[x_reg] = self.registers[y_reg];
                    },
                    0x1 => {
                        self.registers[x_reg] = self.registers[x_reg] | self.registers[y_reg]; 
                    },
                    0x2 => {
                        self.registers[x_reg] = self.registers[x_reg] & self.registers[y_reg]; 
                    },
                    0x3 => {
                        self.registers[x_reg] = self.registers[x_reg] ^ self.registers[y_reg]; 
                    },
                    0x4 => {
                        if (self.registers[x_reg] as usize) + (self.registers[y_reg] as usize) > 255 {
                            self.registers[0xF] = 0x1;
                        } else {
                            self.registers[0xF] = 0x0;
                        }
                        self.registers[x_reg] = self.registers[x_reg].wrapping_add(self.registers[y_reg]);
                    },
                    0x5 => {
                        if (self.registers[x_reg] as i32) - (self.registers[y_reg] as i32) > 0 {
                            self.registers[0xF] = 0x0;
                        } else {
                            self.registers[0xF] = 0x1;
                        }
                        self.registers[x_reg] = self.registers[x_reg].wrapping_sub(self.registers[y_reg]);
                    },
                    0x6 => {
                        let msb = self.registers[x_reg] >> 7 & 0x1;
                        self.registers[0xF] = msb;
                        self.registers[x_reg] = self.registers[y_reg] >> 1;
                    },
                    0x7 => {
                        panic!(format!("Invalid opcode {:#04x}", opcode));
                    },
                    0xE => {
                        let msb = self.registers[x_reg] >> 7 & 0x1;
                        self.registers[0xF] = msb;
                        self.registers[x_reg] = self.registers[y_reg] << 1;
                    },
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                };
            },
            0x9000..0x9FFF => {
                let x_reg = ((opcode >> 8) & 0xF) as usize;
                let y_reg = ((opcode >> 4) & 0xF) as usize;
                let instruction = (opcode & 0xF) as usize;
                match instruction {
                    0x0 => if self.registers[x_reg] != self.registers[y_reg] {
                        self.pc += 2; 
                    },
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                };
            },
            0xA000..0xAFFF => {
                self.index_register = opcode & 0xFFF;
            },
            0xC000..0xCFFF => {
                let x_reg = ((opcode >> 8) & 0xF) as usize;
                let rando: u8 = rand::thread_rng().gen();
                self.registers[x_reg] = rando & ((opcode & 0xFF) as u8);
            },
            0xD000..0xDFFF => {
                self.draw_flag = true;
                let x_reg = ((opcode >> 8) & 0xF) as usize;
                let y_reg = ((opcode >> 4) & 0xF) as usize;
                let x_coord = self.registers[x_reg] as usize;
                let y_coord = self.registers[y_reg] as usize;
                let num_bytes = (opcode & 0xF) as usize;
                let offset = self.index_register as usize;
                for i in 0..num_bytes {
                    let pixel = self.memory[offset + i];
                    for x in 0..8 {
                        if pixel & (0x80 >> x) != 0 {
                            let offset = (x_coord + x)+(PIXEL_WIDTH*(y_coord + i));
                            self.gfx[offset] ^= 1;
                        }
                    }
                }
            },
            0xE000..0xEFFF => {
                let offset = ((opcode >> 8) & 0xF) as usize;
                let instruction = (opcode & 0xFF) as usize;
                match instruction {
                    0x9E => {
                        if (self.keys >> offset) & 0x1 != 0 {
                            self.pc += 2; 
                        }
                    },
                    0xA1 => {
                        println!("keys {:#04x}", self.keys);
                        if (self.keys >> offset) & 0x1 == 0 {
                            self.pc += 2; 
                            //pause();
                        }
                    },
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                }
            },
            0xF000..0xFFFF => {
                let x_reg = ((opcode >> 8) & 0xF) as usize;
                let instruction = (opcode & 0xFF) as usize;
                match instruction {
                    0x07 => {
                        self.registers[x_reg] = self.delay_timer;
                    },
                    0x0A => {
                        panic!(format!("Invalid opcode {:#04x}", opcode));
                    },
                    0x15 => {
                        self.delay_timer = self.registers[x_reg];
                    },
                    0x18 => {
                        self.sound_timer = self.registers[x_reg];
                    },
                    0x1E => {
                        self.index_register += self.registers[x_reg] as u16;
                    },
                    0x29 => {
                        self.index_register = self.registers[x_reg] as u16;
                    },
                    0x33 => {
                        let value = self.registers[x_reg];
                        self.memory[self.index_register as usize] = value / 100;
                        self.memory[self.index_register as usize + 1] = (value / 10) % 10;
                        self.memory[self.index_register as usize + 2] = value % 10;
                    },
                    0x55 => {
                        for x in 0..x_reg {
                            self.memory[(self.index_register as usize) + x] = self.registers[x];
                        }
                        self.index_register = self.index_register + (x_reg as u16) + 1;
                    },
                    0x65 => {
                        for x in 0..x_reg {
                            self.registers[x] = self.memory[(self.index_register as usize) + x];
                        }
                        self.index_register = self.index_register + (x_reg as u16) + 1;
                    },
                    _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
                }
            }
            
            _ => panic!(format!("Invalid opcode {:#04x}", opcode)),
        }

        if self.sound_timer > 0 {
            self.sound_timer -= 1;
        }

        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
    }

	fn set_key(&mut self, key: u8) {
		self.keys |= (0x1 << (key - 1));	
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

    window.limit_update_rate(Some(std::time::Duration::from_millis(2)));

    chip8.load_rom(rom_name).unwrap();

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
					Key::X => chip8.set_key(0xB),
					Key::C => chip8.set_key(0xC),
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

        window
            .update_with_buffer(&display_buf, PIXEL_WIDTH, PIXEL_HEIGHT)
            .unwrap();

        chip8.clear_keys();
    }
}
