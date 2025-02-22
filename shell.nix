{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
    nativeBuildInputs = with pkgs;
        [rustPlatform.bindgenHook pkg-config pipewire SDL2 SDL2_gfx fftw fftwFloat];
}
