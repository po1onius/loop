;; Shared Android, Rust and Node development environment:
;; guix shell -L guix/modules -m guix/env.scm
;; Use -C -N -F for Gradle/npm downloads containing upstream Linux binaries.
(use-modules (guix profiles)
             (gnu packages)
             (loop android))

(concatenate-manifests
 (list
  (packages->manifest (list android-sdk-with-emulator android-jdk))
  (specifications->manifest
   '("bash" "coreutils" "findutils" "grep" "sed" "gawk" "which"
     "git-minimal" "make" "curl" "unzip" "zip"
     ;; Rust's cargo and tools (rustfmt/clippy) are separate Guix outputs.
     "rust" "rust:cargo" "rust:tools"
     "node" "python"
     ;; Native dependencies for Diesel/libpq, TLS and aws-lc-sys.
     ;; Clear host include/library search paths only for Android NDK commands;
     ;; native Rust and Node builds need these paths (see README.md).
     "gcc-toolchain" "cmake" "pkg-config" "openssl" "postgresql"
     "zlib" "nss-certs"))))
