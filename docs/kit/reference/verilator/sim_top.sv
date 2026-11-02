// EXAMPLE from V-Bear (Funtech Super A'Can, MiSTer core simulated with Verilator). It is a
// starting point: names, paths and the system's own details (BIOS loading, cartridge, video
// size, RAM port) must be adapted. See docs/kit-docs.md, "Validation with a reference".

// Top module for the simulation: the Super A'Can console of the MiSTer core without the
// MiSTer framework. Cartridge ROM is a plain array filled by sim.h, read through the
// same request/busy/done contract the SDRAM bridge gives the core.

module sim_top #(
	parameter int CART_LATENCY = 12   // clk_sys cycles from req to done
) (
	input  wire        clk_sys,
	input  wire        cold_reset,

	input  wire [11:0] pad,           // framework order: right left down up A B X Y L R select start
	input  wire        cart_present,
	input  wire [24:0] rom_size,

	// work RAM readback through the savestate port (only used after the run)
	input  wire        wram_own,
	input  wire [14:0] wram_addr,
	output wire [15:0] wram_q,

	input  wire        ld_wr,
	input  wire [14:0] ld_addr,
	input  wire [15:0] ld_data,

	output wire        ce_pix,
	output wire [7:0]  vid_r,
	output wire [7:0]  vid_g,
	output wire [7:0]  vid_b,
	output wire        vid_hs,
	output wire        vid_vs,
	output wire        vid_de,
	output wire [15:0] audio_l,
	output wire [15:0] audio_r
);
	// 16 MB of cartridge, as words; filled from C++.
	reg [15:0] rom [0:8388607] /* verilator public_flat_rw */;

	wire        cart_mem_req;
	wire [25:0] cart_mem_addr;
	reg  [15:0] cart_mem_dout = 16'd0;
	reg         cart_mem_busy = 1'b0;
	reg         cart_mem_done = 1'b0;
	reg  [7:0]  cnt = 8'd0;
	reg  [25:0] addr_q = 26'd0;

	always @(posedge clk_sys) begin
		cart_mem_done <= 1'b0;
		if (cart_mem_busy) begin
			cnt <= cnt - 8'd1;
			if (cnt == 8'd1) begin
				cart_mem_busy <= 1'b0;
				cart_mem_done <= 1'b1;
				cart_mem_dout <= rom[addr_q[23:1]];
			end
		end else if (cart_mem_req && !cart_mem_done) begin
			cart_mem_busy <= 1'b1;
			addr_q <= cart_mem_addr;
			cnt <= CART_LATENCY[7:0];
		end
	end

	wire [47:0] pads = {36'd0, pad};

	superacan_console console (
		.clk_sys(clk_sys), .cold_reset(cold_reset), .reset_btn_n(1'b1),
		.multitap(2'd0), .pad1(pads), .pad2(24'd0),
		.pause_cpu(1'b0), .pause_vid(1'b0),
		.ss_cpu_ready(), .ss_snd_ready(), .vid_overscan(),
		.ss_sndram_own(1'b0), .ss_sndram_addr(16'd0), .ss_sndram_we(1'b0), .ss_sndram_wdata(8'd0),
		.ss_sndram_q(), .ss_mem_idle(), .ss_vid_ready(),
		.ss_din(64'd0), .ss_addr(10'd0), .ss_wren(1'b0), .ss_rst(1'b0), .ss_load(1'b0), .ss_dout(),
		.ss_wram_own(wram_own), .ss_wram_addr(wram_addr), .ss_wram_we(1'b0), .ss_wram_be(2'd0), .ss_wram_wdata(16'd0), .ss_wram_q(wram_q),
		.ss_vram_own(1'b0), .ss_vram_addr(16'd0), .ss_vram_we(1'b0), .ss_vram_be(2'd0), .ss_vram_wdata(16'd0), .ss_vram_q(),
		.ss_pal_own(1'b0), .ss_pal_addr(8'd0), .ss_pal_we(1'b0), .ss_pal_be(2'd0), .ss_pal_wdata(16'd0), .ss_pal_q(),
		.ss_cartram_own(1'b0), .ss_cartram_addr(15'd0), .ss_cartram_we(1'b0), .ss_cartram_wdata(8'd0), .ss_cartram_q(),
		.bk_addr(14'd0), .bk_wren(1'b0), .bk_be(2'd0), .bk_wdata(16'd0), .bk_q(), .bk_dirty(),
		.cheat_load_begin(1'b0), .cheat_invalidate(1'b0), .cheat_commit_req(1'b0), .cheat_paused(1'b0),
		.cheat_code(129'd0), .cheat_commit_done(),
		.ld_wr(ld_wr), .ld_addr(ld_addr), .ld_data(ld_data),
		.cart_mem_req(cart_mem_req), .cart_mem_addr(cart_mem_addr), .cart_mem_dout(cart_mem_dout),
		.cart_mem_busy(cart_mem_busy), .cart_mem_done(cart_mem_done),
		.cart_present(cart_present), .rom_size(rom_size),
		.ce_pix(ce_pix), .vid_r(vid_r), .vid_g(vid_g), .vid_b(vid_b),
		.vid_hs(vid_hs), .vid_vs(vid_vs), .vid_hb(), .vid_vb(), .vid_de(vid_de),
		.audio_l(audio_l), .audio_r(audio_r)
	);
endmodule
