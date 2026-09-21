{
  description = "librarian-store";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
           default = pkgs.rustPlatform.buildRustPackage {
           pname = "librarian-store";
           version = "0.1.0";
           cargoLock.lockFile = ./Cargo.lock;
           nativeBuildInputs = [ pkgs.makeWrapper ];
           buildInputs = [ pkgs.poppler-utils ];
           src = builtins.path {
             path = ./.;
             name = "librarian-store-source";
             filter = path: type:
               let name = pkgs.lib.removePrefix "${toString ./.}/" (toString path);
               in !(pkgs.lib.hasInfix "/target/" name) && name != "target" && name != ".git";
           };
           postInstall = ''
             wrapProgram $out/bin/librarian-store --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.poppler-utils ]}
           '';
        };
      });
      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/librarian-store";
        };
      });
    };
}
