# L'ATELIER DE CONSTRUCTION LINUX DU MONTEUR (22/11/2026). Il part de celui de VerdierCAM (outils/linux/Dockerfile de
# verdiercam-imgui : Ubuntu 22.04, glibc 2.35, gcc 12, cmake, patchelf, appimagetool) et n'y ajoute que ce que le
# Monteur demande en plus : clang pour bindgen (les en-têtes de FFmpeg), et Rust 1.100.0. Construit ici, sur CachyOS,
# le Monteur exigerait la glibc 2.43 : d'ici, il démarre sur toute distribution de 2022 ou après.
FROM verdiercam-construction-linux

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
RUN curl -fsSL -o /tmp/rustup-init https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init \
 && curl -fsSL -o /tmp/rustup-init.sha256 https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256 \
 && [ "$(cut -d' ' -f1 /tmp/rustup-init.sha256)" = "$(sha256sum /tmp/rustup-init | cut -d' ' -f1)" ] \
 && chmod +x /tmp/rustup-init \
 && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.100.0 \
 && rm /tmp/rustup-init /tmp/rustup-init.sha256 \
 && chmod -R a+rX /opt/rustup /opt/cargo
ENV PATH=/opt/cargo/bin:$PATH
