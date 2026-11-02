// EXAMPLE from V-Bear (Funtech Super A'Can, MiSTer core simulated with Verilator). It is a
// starting point: names, paths and the system's own details (BIOS loading, cartridge, video
// size, RAM port) must be adapted. See docs/kit-docs.md, "Validation with a reference".

// The MiSTer core simulated with Verilator: reset, BIOS/cartridge load, video capture, frame loop.
#pragma once
#include "Vsim_top.h"
#include "Vsim_top___024root.h"
#include "verilated.h"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <vector>

struct Sim {
	Vsim_top *top;
	Vsim_top___024root *r;
	uint64_t cycles = 0;

	// video: the last complete frame
	static constexpr int MAXW = 512, MAXH = 512;
	std::vector<uint32_t> cur = std::vector<uint32_t>(MAXW * MAXH, 0);
	std::vector<uint32_t> last = std::vector<uint32_t>(MAXW * MAXH, 0);
	int x = 0, y = 0, cur_w = 0, last_w = 0, last_h = 0;
	bool prev_hs = false, prev_vs = false, prev_de = false;
	uint64_t frames_seen = 0;

	explicit Sim(VerilatedContext *ctx) : top(new Vsim_top(ctx)), r(top->rootp) {
		top->clk_sys = 0;
		top->cold_reset = 1;
		top->pad = 0;
		top->cart_present = 0;
		top->rom_size = 0;
		top->ld_wr = 0;
		top->wram_own = 0; top->wram_addr = 0;
		top->eval();
	}
	~Sim() { top->final(); delete top; }

	void video_sample() {
		bool hs = top->vid_hs, vs = top->vid_vs, de = top->vid_de;
		if (top->ce_pix) {
			if (de && x < MAXW && y < MAXH) cur[y * MAXW + x++] = (top->vid_r << 16) | (top->vid_g << 8) | top->vid_b;
		}
		if (prev_de && !de) { cur_w = std::max(cur_w, x); x = 0; y++; }   // end of an active line
		if (vs && !prev_vs) {                                             // vertical sync: frame done
			last.swap(cur); last_w = cur_w; last_h = y; frames_seen++;
			cur_w = 0; x = 0; y = 0;
		}
		prev_hs = hs; prev_vs = vs; prev_de = de;
	}

	inline void tick() {
		top->clk_sys = 1; top->eval();
		top->clk_sys = 0; top->eval();
		video_sample();
		cycles++;
	}

	// BIOS image: 4096 (68000 IPL) + 8192 + 8192 (6502) + 16 (key), as in boot.rom.
	// Dumps carry each 68000 word byte-swapped and the HPS sends words low byte first,
	// so the word the core sees is file[2i] | file[2i+1] << 8.
	void load_bios(const std::vector<uint8_t> &b) {
		top->cold_reset = 1;
		for (size_t i = 0; i + 1 < b.size() && i < 0x5010; i += 2) {
			top->ld_wr = 1; top->ld_addr = (uint16_t)i;
			top->ld_data = b[i] | (b[i + 1] << 8);
			tick();
		}
		top->ld_wr = 0;
	}

	// The cartridge as 68000 words. Two byte orders exist; the vector table tells them apart.
	void load_cart(const std::vector<uint8_t> &rom) {
		auto score = [&](bool be) {
			int n = 0;
			for (int v = 0; v < 64 && (size_t)(v * 4 + 3) < rom.size(); v++) {
				const uint8_t *p = &rom[v * 4];
				uint32_t a = be ? (p[0] << 24 | p[1] << 16 | p[2] << 8 | p[3])
				                : (p[1] << 24 | p[0] << 16 | p[3] << 8 | p[2]);
				if (!(a & 1) && a < 0x1000000) n++;
			}
			return n;
		};
		bool be = score(true) >= score(false);
		fprintf(stderr, "cart: %zu bytes, %s byte order\n", rom.size(), be ? "big-endian" : "word-swapped");
		size_t words = std::min<size_t>(rom.size() / 2, 8388608);
		for (size_t i = 0; i < words; i++)
			r->sim_top__DOT__rom[i] = be ? (rom[2 * i] << 8 | rom[2 * i + 1]) : (rom[2 * i + 1] << 8 | rom[2 * i]);
		top->rom_size = (uint32_t)rom.size();
		top->cart_present = 1;
	}

	// The 64 KB of work RAM, as the 68000 sees it (big-endian: even byte = high lane).
	std::vector<uint8_t> dump_wram() {
		std::vector<uint8_t> out(0x10000);
		top->wram_own = 1;
		for (int k = 0; k < 0x8000; k++) {
			top->wram_addr = k;
			tick(); tick();
			out[2 * k] = top->wram_q >> 8; out[2 * k + 1] = top->wram_q & 0xFF;
		}
		top->wram_own = 0;
		return out;
	}

	void reset() {
		top->cold_reset = 1;
		for (int i = 0; i < 2000; i++) tick();
		top->cold_reset = 0;
	}

	// Runs until the next vertical sync completes a frame. Capped so it cannot hang.
	bool run_frame() {
		uint64_t target = frames_seen + 1;
		for (uint64_t i = 0; i < 4000000 && frames_seen < target; i++) tick();
		return frames_seen >= target;
	}
};
