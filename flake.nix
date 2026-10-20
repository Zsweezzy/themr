{
  description = "themr — one window to set the color theme of every app on your desktop";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: {
        themr = pkgs.rustPlatform.buildRustPackage {
          pname = "themr";
          version = "0.1.0";
          # self, not ./. — so the cargo build sees only tracked files, not target/
          src = self;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.wrapGAppsHook4
          ];
          buildInputs = [ pkgs.gtk4 ];

          meta = {
            description = "One window to set the color theme of every app on your desktop";
            homepage = "https://github.com/Zsweezzy/themr";
            license = pkgs.lib.licenses.mit;
            mainProgram = "themr";
            platforms = pkgs.lib.platforms.linux;
          };
        };
        default = self.packages.${pkgs.system}.themr;
      });

      # cargo test / cargo clippy with GTK on the path: nix develop
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.system}.themr ];
          packages = [ pkgs.rust-analyzer ];
        };
      });
    };
}
