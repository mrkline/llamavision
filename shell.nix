{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
    nativeBuildInputs = with pkgs;
        [rustPlatform.bindgenHook pkg-config pipewire sdl3 fftw fftwFloat];
}
