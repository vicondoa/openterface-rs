# Provides pre-built openterface-rs binaries from GitHub Releases.
# Returns a derivation when a release manifest exists, null otherwise.
{ pkgs, lib ? pkgs.lib }:

let
  manifest = builtins.fromJSON (builtins.readFile ./prebuilt.json);
  hasBinary =
    manifest.version != null
    && builtins.hasAttr "openterface-rs" manifest.binaries;
in
if hasBinary then
  pkgs.stdenv.mkDerivation {
    pname = "openterface-rs";
    version = manifest.version;

    src = pkgs.fetchurl {
      inherit (manifest.binaries."openterface-rs") url hash;
    };

    nativeBuildInputs = [ pkgs.makeWrapper ];
    dontConfigure = true;
    dontBuild = true;

    installPhase = ''
      runHook preInstall
      mkdir -p $out/bin $out/lib/udev/rules.d
      install -Dm755 openterface-rs $out/bin/openterface-rs
      if [ -f 60-openterface.rules ]; then
        install -Dm0644 60-openterface.rules $out/lib/udev/rules.d/60-openterface.rules
      fi
      runHook postInstall
    '';

    # Runtime deps that the binary dlopens.
    postInstall = ''
      wrapProgram $out/bin/openterface-rs \
        --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath (with pkgs; [
          udev
          libv4l
          wayland
          libxkbcommon
          libdecor
          vulkan-loader
          libGL
        ])}"
    '';

    meta = with lib; {
      description = "Openterface Mini-KVM controller (pre-built binary)";
      homepage = "https://github.com/vicondoa/openterface-rs";
      license = licenses.asl20;
      platforms = [ manifest.system ];
      mainProgram = "openterface-rs";
    };
  }
else
  null
