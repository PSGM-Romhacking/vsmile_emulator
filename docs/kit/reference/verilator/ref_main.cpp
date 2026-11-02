// EXAMPLE from V-Bear (Funtech Super A'Can, MiSTer core simulated with Verilator). It is a
// starting point: names, paths and the system's own details (BIOS loading, cartridge, video
// size, RAM port) must be adapted. See docs/kit-docs.md, "Validation with a reference".

// Reference: the MiSTer core of the Super A'Can simulated with Verilator, without a window.
//   acan-ref GAME.bin --bios DIR --shot FRAMES out.ppm [--dump-ram ram.bin]
// DIR holds the four BIOS files (internal_68k.bin, internal_6502_1.bin, internal_6502_2.bin, umc6650.bin).
#include "sim.h"
#include <string>
#include <unistd.h>
#include <climits>
#include <libgen.h>

static std::vector<uint8_t> read_file(const std::string &p) {
	FILE *f = fopen(p.c_str(), "rb");
	if (!f) { perror(p.c_str()); exit(1); }
	std::vector<uint8_t> d; uint8_t buf[4096]; size_t n;
	while ((n = fread(buf, 1, sizeof buf, f)) > 0) d.insert(d.end(), buf, buf + n);
	fclose(f);
	return d;
}

static void write_ppm(const Sim &sim, const std::string &path) {
	FILE *f = fopen(path.c_str(), "wb");
	fprintf(f, "P6\n%d %d\n255\n", sim.last_w, sim.last_h);
	for (int y = 0; y < sim.last_h; y++) for (int x = 0; x < sim.last_w; x++) {
		uint32_t p = sim.last[y * Sim::MAXW + x];
		uint8_t rgb[3] = {(uint8_t)(p >> 16), (uint8_t)(p >> 8), (uint8_t)p};
		fwrite(rgb, 1, 3, f);
	}
	fclose(f);
}

int main(int argc, char **argv) {
	const char *rom = nullptr, *shot = nullptr, *dump = nullptr, *bios = "bios";
	int frames = 600, first = -1, last = -1; const char *prefix = nullptr;
	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--shot") && i + 2 < argc) { frames = atoi(argv[++i]); shot = argv[++i]; }
		else if (!strcmp(argv[i], "--frames-out") && i + 3 < argc) { first = atoi(argv[++i]); last = atoi(argv[++i]); prefix = argv[++i]; frames = std::max(frames == 600 ? 0 : frames, last); }
		else if (!strcmp(argv[i], "--dump-ram") && i + 1 < argc) dump = argv[++i];
		else if (!strcmp(argv[i], "--bios") && i + 1 < argc) bios = argv[++i];
		else rom = argv[i];
	}
	if (!rom) { fprintf(stderr, "usage: %s GAME [--bios DIR] [--shot N out.ppm] [--dump-ram ram.bin]\n", argv[0]); return 1; }
	// fx68k loads microrom.mem and nanorom.mem from the current directory: go to the
	// binary's folder, after turning the paths given into absolute ones
	auto absolute = [](const char *p) { char b[PATH_MAX]; return std::string(realpath(p, b) ? b : p); };
	std::string romp = absolute(rom), biosp = absolute(bios), shotp = shot ? std::string(shot) : "", dumpp = dump ? std::string(dump) : "";
	auto absout = [](std::string &p) { if (!p.empty() && p[0] != '/') { char b[PATH_MAX]; p = std::string(getcwd(b, sizeof b)) + "/" + p; } };
	absout(shotp); absout(dumpp);
	{ char self[PATH_MAX]; ssize_t n = readlink("/proc/self/exe", self, sizeof self - 1); if (n > 0) { self[n] = 0; if (chdir(dirname(self))) perror("chdir"); } }
	rom = romp.c_str(); shot = shot ? shotp.c_str() : nullptr; dump = dump ? dumpp.c_str() : nullptr;
	std::string d = biosp;
	std::vector<uint8_t> image;
	for (const char *n : {"internal_68k.bin", "internal_6502_1.bin", "internal_6502_2.bin", "umc6650.bin"}) {
		auto f = read_file(d + "/" + n);
		image.insert(image.end(), f.begin(), f.end());
	}
	auto ctx = std::make_unique<VerilatedContext>();
	Sim sim(ctx.get());
	sim.load_bios(image);
	sim.load_cart(read_file(rom));
	sim.reset();
	for (int f = 0; f < frames; f++)
	{
		if (!sim.run_frame()) { fprintf(stderr, "no frame at %d\n", f); break; }
		if (prefix && f + 1 >= first && f + 1 <= last) write_ppm(sim, std::string(prefix) + (f + 1 < 10 ? "00" : f + 1 < 100 ? "0" : "") + std::to_string(f + 1) + ".ppm");
	}
	fprintf(stderr, "frames=%llu size=%dx%d cycles=%llu\n", (unsigned long long)sim.frames_seen, sim.last_w, sim.last_h, (unsigned long long)sim.cycles);
	if (shot) {
		write_ppm(sim, shot);
	}
	if (dump) {
		auto ram = sim.dump_wram();
		FILE *f = fopen(dump, "wb");
		fwrite(ram.data(), 1, ram.size(), f);
		fclose(f);
	}
	return 0;
}
