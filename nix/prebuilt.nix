# Pre-built openterface-rs from GitHub Releases.
# Uses autoPatchelfHook to fix ELF paths for any consumer's nixpkgs.
# Returns a derivation when release hashes exist, null otherwise.
{ pkgs, lib ? pkgs.lib }:

let
  manifest = builtins.fromJSON (builtins.readFile ./prebuilt.json);
  hasBinary =
    manifest.version != null
    && builtins.hasAttr "openterface-rs" manifest.binaries;
  runtimeLibs = with pkgs; [
    stdenv.cc.cc.lib udev libv4l wayland libxkbcommon libdecor
    vulkan-loader libGL
  ];
in
if hasBinary then
  pkgs.stdenv.mkDerivation {
    pname = "openterface-rs";
    version = manifest.version;
    src = pkgs.fetchurl {
      inherit (manifest.binaries."openterface-rs") url hash;
    };
    nativeBuildInputs = with pkgs; [ autoPatchelfHook makeWrapper ];
    buildInputs = runtimeLibs;
    sourceRoot = ".";
    dontConfigure = true;
    dontBuild = true;
    installPhase = ''
      runHook preInstall
      dir=$(find . -maxdepth 1 -type d -name "openterface-rs-*" | head -1)
      if [ -n "$dir" ]; then
        install -Dm755 "$dir/openterface-rs" $out/bin/openterface-rs
        if [ -f "$dir/60-openterface.rules" ]; then
          install -Dm644 "$dir/60-openterface.rules" $out/lib/udev/rules.d/60-openterface.rules
        fi
      else
        install -Dm755 openterface-rs $out/bin/openterface-rs
        [ -f 60-openterface.rules ] && install -Dm644 60-openterface.rules $out/lib/udev/rules.d/60-openterface.rules
      fi
      runHook postInstall
    '';
    postFixup = ''
      wrapProgram $out/bin/openterface-rs \
        --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}"
    '';
    meta = {
      description = "Openterface Mini-KVM controller (pre-built)";
      mainProgram = "openterface-rs";
      platforms = [ manifest.system ];
    };
  }
else
  null
