;; Development package for `guix shell -C -N -F -D -f guix.scm`.
;; Keep this file a package (not a manifest) so interactive `guix shell -C`
;; can automatically load its development inputs after directory authorization.
(use-modules (guix packages)
             (guix gexp)
             (guix build-system trivial)
             (gnu packages)
             (gnu packages rust))

(package
  (name "loop-development")
  (version "0.1.0")
  (source #f)
  (build-system trivial-build-system)
  ;; This package describes the shell; Cargo and npm build the application.
  (arguments (list #:builder #~(mkdir #$output)))
  (native-inputs
   (append
    ;; Guix separates rustc, Cargo, and rustfmt/Clippy into different outputs.
    (list rust (list rust "cargo") (list rust "tools"))
    (map specification->package
         '("bash" "coreutils" "findutils" "grep" "sed" "gawk"
           "git-minimal" "make"
           ;; Native crates (aws-lc-sys/ring) and npm native dependencies.
           "gcc-toolchain" "pkg-config" "cmake" "perl" "python"
           ;; libpq/pg_config for Diesel CLI; TLS headers and certificate roots.
           "postgresql" "openssl" "nss-certs"
           ;; Includes npm and npx for the Expo app.
           "node"))))
  (home-page "")
  (synopsis "Development environment for LOOP")
  (description
   "Build tools for the LOOP Rust backend and Expo JavaScript application.
Database and messaging services are managed separately with Podman Compose.")
  (license #f))
