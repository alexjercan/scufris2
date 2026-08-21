{pkgs}: let
  piperPackage =
    (pkgs.piper-tts.override {
      withTrain = false;
      withHTTP = false;
      withAlignment = false;
    }).overrideAttrs (old: {
      pname = "scufris-piper-tts";
      patches = (old.patches or []) ++ [./piper-stdout.patch];
    });
  model = pkgs.fetchurl {
    name = "en_US-lessac-medium.onnx";
    url = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx";
    hash = "sha256-Xv4J5pkCGHgnr2RuGm6dJp3udp+Yd9F7FrG0buqvAZ8=";
  };
  config = pkgs.fetchurl {
    name = "en_US-lessac-medium.onnx.json";
    url = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json";
    hash = "sha256-7+GcQXvtBV8taZCCSMa6ZQ+hNbyGiw5quz2hgdq2kKA=";
  };
  assets = pkgs.runCommand "scufris-voice-en_US-lessac-medium" {} ''
    mkdir -p "$out/share/scufris/voices"
    ln -s ${model} "$out/share/scufris/voices/en_US-lessac-medium.onnx"
    ln -s ${config} "$out/share/scufris/voices/en_US-lessac-medium.onnx.json"
  '';
in {
  inherit assets piperPackage;
  model = "${assets}/share/scufris/voices/en_US-lessac-medium.onnx";
  config = "${assets}/share/scufris/voices/en_US-lessac-medium.onnx.json";
}
