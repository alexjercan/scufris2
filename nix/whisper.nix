{pkgs}: {
  # Multilingual base model. Small enough to ship as the always-available
  # default; nix.dotfiles overrides it with the larger pinned model.
  model = pkgs.fetchurl {
    name = "ggml-base.bin";
    url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin";
    hash = "sha256-YO1bw90U7qhWST0zQ0m0BXgt3K8AKNS130CINF+6Lv4=";
  };

  package = pkgs.whisper-cpp;

  host = "127.0.0.1";
  port = 10302;
  inferencePath = "/inference";
}
