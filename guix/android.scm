;; guix shell -L guix/modules -m guix/android.scm
;; Use -C -N -F for Gradle/npm downloads containing upstream Linux binaries.
(use-modules (guix profiles)
             (gnu packages)
             (gnu packages gcc)
             (loop android))

;; FHS binaries need libstdc++, but GCC's header/library search variables would
;; inject host glibc headers into Clang's Android sysroot during cross-compilation.
(define gcc-runtime
  (manifest-entry
    (inherit (package->manifest-entry gcc "lib"))
    (search-paths '())))

(concatenate-manifests
 (list
  (packages->manifest (list android-sdk-with-emulator android-jdk))
  (manifest (list gcc-runtime))
  (specifications->manifest
   '("bash" "coreutils" "findutils" "grep" "sed" "gawk" "which"
     "git-minimal" "make" "node" "python" "unzip" "zip"
     "zlib" "nss-certs"))))
