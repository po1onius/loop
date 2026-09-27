;; Android SDK composition for x86_64-linux.
;; Design reference: nixpkgs/pkgs/development/mobile/androidenv.
(define-module (loop android)
  #:use-module (gnu packages)
  #:use-module (gnu packages base)
  #:use-module (gnu packages compression)
  #:use-module (gnu packages elf)
  #:use-module (gnu packages gcc)
  #:use-module (guix base16)
  #:use-module (guix build-system copy)
  #:use-module (guix build-system trivial)
  #:use-module (guix download)
  #:use-module (guix gexp)
  #:use-module ((guix licenses) #:prefix license:)
  #:use-module (guix packages)
  #:use-module (guix search-paths)
  #:use-module (guix utils)
  #:use-module (loop android-sources)
  #:export (android-jdk
            android-command-line-tools android-platform-tools
            android-platform-36 android-build-tools-35 android-build-tools-36 android-ndk
            android-cmake android-emulator android-system-image-36
            android-sdk android-sdk-with-emulator))

(define %sdk-directory "lib/android-sdk")

(define %android-build-modules
  '((guix build copy-build-system) (guix build utils) (guix elf)
    (rnrs io ports) (ice-9 popen) (srfi srfi-1)))

(define (packages names)
  (map (lambda (name)
         (call-with-values
             (lambda () (specification->package+output name))
           (lambda (package output)
             (if (string=? output "out") package (list package output)))))
       names))

(define %native-libraries
  (append (list glibc (list gcc "lib"))
          (packages '("zlib" "libcxx"))))

;; Apply fixups only to explicitly selected host directories.  In particular,
;; NDK sysroots and emulator guest images must never be patched or stripped.
(define* (patch-host-tools directories bundled-libraries libraries
                           #:optional (extra-library-paths '()))
  #~(begin
      (use-modules (guix build utils) (guix elf)
                   (rnrs io ports) (ice-9 popen)
                   (srfi srfi-1))
      (let* ((roots (map (lambda (dir) (string-append #$output "/" dir))
                         '#$directories))
             (rpaths (append
                      (map (lambda (dir) (string-append #$output "/" dir))
                           '#$bundled-libraries)
                      (list #$@(map (lambda (lib)
                                      #~(string-append
                                         #$(if (pair? lib)
                                               (gexp-input (car lib) (cadr lib))
                                               lib)
                                         "/lib")) libraries))
                      (list #$@extra-library-paths)))
             (interpreter #$(file-append glibc "/lib/ld-linux-x86-64.so.2")))
        (for-each
         (lambda (file)
           (let* ((elf (call-with-input-file file
                         (lambda (port) (parse-elf (get-bytevector-all port)))))
                  (segments (elf-segments elf)))
             (when (and (= (elf-machine-type elf) EM_X86_64)
                        (any (lambda (segment)
                               (= (elf-segment-type segment) PT_DYNAMIC))
                             segments))
               (format #t "android: patching host ELF ~a~%" file)
               (when (any (lambda (segment)
                            (= (elf-segment-type segment) PT_INTERP)) segments)
                 (invoke "patchelf" "--set-interpreter" interpreter file))
               (let* ((port (open-pipe* OPEN_READ "patchelf" "--print-rpath" file))
                      (original (string-trim-right (get-string-all port))))
                 (unless (zero? (close-pipe port))
                   (error "Cannot read ELF RPATH" file))
                 (invoke "patchelf" "--set-rpath"
                         (string-join
                          (delete-duplicates
                           (append (filter (lambda (s) (not (string-null? s)))
                                           (string-split original #\:))
                                   rpaths)) ":")
                         file)))))
         (delete-duplicates
          (append-map (lambda (root)
                        (find-files root (lambda (file stat) (elf-file? file))))
                      roots))))))

(define android-jdk
  (package
    (name "android-temurin-jdk")
    (version "17.0.20.1-1")
    (source
     (origin
       (method url-fetch)
       (uri "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.20.1%2B1/OpenJDK17U-jdk_x64_linux_hotspot_17.0.20.1_1.tar.gz")
       (sha256
        (base16-string->bytevector "3808d1d15e3ec6bd5b84057fb5d84c33d8a1536a258146bcea2e603fc726e08e"))))
    (build-system copy-build-system)
    (arguments
     (list
      #:substitutable? #f ; Local repackaging; dependencies still use substitutes.
      #:modules %android-build-modules
      #:strip-binaries? #f
      #:install-plan ''(("." "lib/jvm/temurin-17"))
      #:phases
      #~(modify-phases %standard-phases
          (add-after 'install 'patch-host-tools
            (lambda _
              #$(patch-host-tools
                 '("lib/jvm/temurin-17/bin" "lib/jvm/temurin-17/lib")
                 '("lib/jvm/temurin-17/lib" "lib/jvm/temurin-17/lib/server")
                 (append (list glibc (list gcc "lib"))
                         (packages '("zlib" "alsa-lib" "fontconfig" "freetype"
                                     "libx11" "libxext" "libxi" "libxrender"
                                     "libxtst"))))))
          (add-after 'patch-host-tools 'expose-java
            (lambda _
              (mkdir-p (string-append #$output "/bin"))
              (for-each
               (lambda (file)
                 (symlink file (string-append #$output "/bin/" (basename file))))
               (find-files (string-append #$output "/lib/jvm/temurin-17/bin")
                           ".*" #:directories? #f)))))))
    ;; Same pin as Nonguix binary-build-system: 0.18 corrupts Android CMake
    ;; executables (verified SIGSEGV); 0.16.1 preserves their ELF layout.
    (native-inputs (list patchelf-0.16))
    (inputs
     (append (list glibc (list gcc "lib"))
             (packages '("zlib" "alsa-lib" "fontconfig" "freetype"
                         "libx11" "libxext" "libxi" "libxrender" "libxtst"))))
    (native-search-paths
     (list (search-path-specification
            (variable "JAVA_HOME")
            (files '("lib/jvm/temurin-17"))
            (separator #f))))
    (supported-systems '("x86_64-linux"))
    (home-page "https://adoptium.net/")
    (synopsis "Maintained Java 17 toolchain for Android builds")
    (description "Eclipse Temurin Java 17, including javac and the tools needed
by the project's Gradle wrapper and Android command-line tools.")
    (license license:gpl2+)))

(define* (android-component key #:key (host-directories '())
                            (bundled-libraries '()) (libraries '())
                            (extra-library-paths '())
                            (fixups #~#t))
  (let* ((metadata (assq-ref %android-sources key))
         (sdk-path (assoc-ref metadata 'path))
         (destination (string-append %sdk-directory "/" sdk-path))
         (root (assoc-ref metadata 'root)))
    (package
      (name (string-append "android-" (symbol->string key)))
      (version (assoc-ref metadata 'version))
      (source
       (origin
         (method url-fetch)
         (uri (assoc-ref metadata 'url))
         (sha256
          (base16-string->bytevector (assoc-ref metadata 'sha256)))))
      (build-system copy-build-system)
      (arguments
       (list
        #:substitutable? #f
        #:modules %android-build-modules
        #:strip-binaries? #f
        #:validate-runpath? #f ; Guest ELF libraries must not be validated as host ELF.
        #:install-plan #~'((#$root #$destination))
        #:phases
        #~(modify-phases %standard-phases
            ;; Use an explicit archive root: GNU unpack's automatic chdir would
            ;; otherwise lose the SDK layout (CMake has several top directories).
            (replace 'unpack
              (lambda* (#:key source #:allow-other-keys)
                (mkdir "source")
                (chdir "source")
                ;; libarchive handles Google's large ZIP64 system images.
                (invoke "bsdtar" "-xf" source)))
            (add-after 'install 'package-metadata
              (lambda _
                (let ((file (string-append #$output "/" #$destination "/package.xml")))
                  (format #t "android: installing metadata for ~a~%" #$sdk-path)
                  (call-with-output-file file
                    (lambda (port)
                      (display #$(string-append
                                  %android-xml-header %android-license
                                  (assoc-ref metadata 'xml)
                                  "</local:repository>") port))))))
            (add-after 'package-metadata 'component-fixups
              (lambda _ #$fixups))
            (add-after 'component-fixups 'patch-host-tools
              (lambda _
                #$(patch-host-tools
                   (map (lambda (dir) (string-append destination "/" dir))
                        host-directories)
                   (map (lambda (dir) (string-append destination "/" dir))
                        bundled-libraries)
                   libraries extra-library-paths))))))
      (native-inputs (list (specification->package "libarchive") patchelf-0.16))
      (inputs libraries)
      (supported-systems '("x86_64-linux"))
      (home-page "https://developer.android.com/studio")
      (synopsis (string-append "Android SDK component: " sdk-path))
      (description "A fixed Android SDK component from Google's repository,
with local package metadata and Guix runtime paths for Linux host tools.")
      (license (license:non-copyleft "https://developer.android.com/studio/terms")))))

(define android-command-line-tools
  (android-component 'command-line-tools
                     #:host-directories '("bin" "lib")
                     #:libraries %native-libraries))

(define android-platform-tools
  (android-component 'platform-tools
                     #:host-directories '(".")
                     #:libraries (append %native-libraries (packages '("libusb")))))

(define android-platform-36 (android-component 'platform))

(define android-build-tools-36
  (android-component 'build-tools
                     #:host-directories '(".")
                     #:bundled-libraries '("lib64")
                     #:libraries %native-libraries))

;; Expo library subprojects use AGP's default, while the app requests 36.0.0.
(define android-build-tools-35
  (android-component 'build-tools-35
                     #:host-directories '(".")
                     #:bundled-libraries '("lib64")
                     #:libraries %native-libraries))

(define android-ndk
  (android-component
   'ndk
   #:host-directories '("toolchains/llvm/prebuilt/linux-x86_64/bin"
                        "toolchains/llvm/prebuilt/linux-x86_64/lib"
                        "prebuilt/linux-x86_64" "shader-tools/linux-x86_64")
   #:bundled-libraries '("toolchains/llvm/prebuilt/linux-x86_64/lib"
                        "shader-tools/linux-x86_64")
   #:libraries %native-libraries))

(define android-cmake
  (android-component 'cmake
                     #:host-directories '("bin")
                     #:libraries %native-libraries))

(define %emulator-libraries
  (append %native-libraries
          (packages '("alsa-lib" "pulseaudio" "libtiff" "util-linux:lib"
                      "libbsd" "libdrm" "expat" "freetype" "fontconfig" "wayland"
                      "libpng"
                      "nss" "nspr" "mesa" "dbus" "eudev" "libx11"
                      "libxext" "libxdamage" "libxfixes" "libxcb"
                      "libxcomposite" "libxcursor" "libxi" "libxrender"
                      "libxtst" "libice" "libsm" "libxkbfile" "libxshmfence"
                      "libxkbcommon" "xcb-util-cursor" "xcb-util-image"
                      "xcb-util-keysyms" "xcb-util-renderutil" "xcb-util-wm"))))

(define android-emulator
  (android-component
   'emulator
   #:host-directories '(".")
   #:bundled-libraries '("lib64" "lib64/qt/lib" "lib64/gles_swiftshader"
                        "lib64/gles_llvmpipe" "lib64/vulkan")
   #:libraries %emulator-libraries
   #:extra-library-paths
   (list (file-append (specification->package "nss") "/lib/nss"))
   ;; androidenv applies the same TIFF SONAME fix to Google's Qt plugin.
   #:fixups
   #~(invoke "patchelf" "--replace-needed" "libtiff.so.5" "libtiff.so"
             (string-append #$output "/lib/android-sdk/emulator/lib64/qt/plugins/imageformats/libqtiffAndroidEmu.so"))))

(define android-system-image-36 (android-component 'system-image))

(define* (compose-android-sdk #:key (emulator? #f))
  (let ((components
         (append (list android-command-line-tools android-platform-tools
                       android-platform-36 android-build-tools-36
                       android-build-tools-35
                       android-ndk android-cmake)
                 (if emulator? (list android-emulator android-system-image-36) '()))))
    (package
      (name (if emulator? "android-sdk-with-emulator" "android-sdk"))
      (version "36")
      (source #f)
      (build-system trivial-build-system)
      (arguments
       (list
        #:modules '((guix build utils) (guix build union))
        #:builder
        #~(begin
            (use-modules (guix build utils) (guix build union) (srfi srfi-1)
                         (ice-9 textual-ports))
            (let* ((sdk (string-append #$output "/" #$%sdk-directory))
                   (bin (string-append #$output "/bin"))
                   (tools (string-append sdk "/cmdline-tools/23.0")))
              (format #t "android: composing SDK with emulator: ~a~%" #$emulator?)
              (mkdir-p (dirname sdk))
              (union-build sdk
                           (list #$@(map (lambda (p) (file-append p "/lib/android-sdk"))
                                         components))
                           #:create-all-directories? #t)
              ;; Since v23 Google's sdkmanager shell script delegates to an
              ;; auto-downloading Android CLI launcher.  Use the SDK Manager
              ;; Java main still shipped in these same jars, with the upstream
              ;; Java launcher, so querying this SDK never bootstraps tools.
              (let ((manager (string-append tools "/bin/sdkmanager")))
                (delete-file manager)
                (copy-file (string-append tools "/bin/avdmanager") manager)
                (substitute* manager
                  (("CLASSPATH=\\$APP_HOME/lib/avdmanager-classpath.jar")
                   "CLASSPATH=$APP_HOME/lib/sdklib/tools.sdklib.jar:$APP_HOME/lib/avdmanager-classpath.jar")
                  (("AvdManagerCli") "sdkmanager.SdkManagerCli")
                  (("AVDMANAGER") "SDKMANAGER")
                  ;; Jar symlinks resolve into component packages; explicitly
                  ;; select the composed SDK instead of relying on discovery.
                  (("exec \"\\$JAVACMD\" \"\\$@\"")
                   (string-append "exec \"$JAVACMD\" \"$@\" --sdk_root=" sdk))))
              ;; Java launchers must live in the composed SDK so APP_HOME and
              ;; avdmanager's SDK discovery refer to the complete installation.
              (for-each
               (lambda (file)
                 (let ((copy (string-append file ".guix-copy")))
                   ;; New command-line tools include a native `android' ELF.
                   ;; Copy bytes, not decoded text, when replacing union links.
                   (copy-file file copy)
                   (delete-file file)
                   (rename-file copy file)
                   (chmod file #o755)
                   (unless (elf-file? file)
                     (substitute* file
                       (("#!/bin/sh") (string-append "#!" #$(file-append (specification->package "bash") "/bin/sh")))))
                   (wrap-program file
                     #:sh #$(file-append (specification->package "bash") "/bin/bash")
                     `("JAVA_HOME" = (#$(file-append android-jdk "/lib/jvm/temurin-17")))
                     `("ANDROID_HOME" = (,sdk))
                     `("PATH" prefix
                       (#$(file-append (specification->package "coreutils") "/bin")
                        #$(file-append (specification->package "findutils") "/bin")
                        #$(file-append (specification->package "sed") "/bin"))))))
               (find-files (string-append tools "/bin") ".*"))
              (when #$emulator?
                (wrap-program (string-append sdk "/emulator/emulator")
                  #:sh #$(file-append (specification->package "bash") "/bin/bash")
                  `("QT_XKB_CONFIG_ROOT" =
                    (#$(file-append (specification->package "xkeyboard-config")
                                    "/share/X11/xkb")))
                  `("QTCOMPOSE" =
                    (#$(file-append (specification->package "libx11")
                                    "/share/X11/locale")))
                  `("QT_QPA_PLATFORM_PLUGIN_PATH" =
                    (,(string-append sdk "/emulator/lib64/qt/plugins")))
                  `("LD_LIBRARY_PATH" prefix
                    (#$(file-append (specification->package "mesa") "/lib")
                     #$(file-append (specification->package "eudev") "/lib")
                     #$(file-append (specification->package "dbus") "/lib")))))
              (mkdir-p bin)
              (for-each
               (lambda (entry)
                 (symlink (string-append sdk "/" (cdr entry))
                          (string-append bin "/" (car entry))))
               (append '(("adb" . "platform-tools/adb")
                         ("fastboot" . "platform-tools/fastboot")
                         ("sdkmanager" . "cmdline-tools/23.0/bin/sdkmanager")
                         ("avdmanager" . "cmdline-tools/23.0/bin/avdmanager"))
                       (if #$emulator? '(("emulator" . "emulator/emulator")) '())))))))
      (native-inputs (packages '("bash" "coreutils" "findutils" "sed")))
      (native-search-paths
       (list (search-path-specification
              (variable "ANDROID_HOME") (files (list %sdk-directory)) (separator #f))))
      (supported-systems '("x86_64-linux"))
      (home-page "https://developer.android.com/studio")
      (synopsis "Composed Android SDK for LOOP")
      (description "Read-only Android SDK with the platform, build tools, NDK
and CMake required by LOOP.  Optional emulator and Google APIs system image.
Install and update components through Guix, not through sdkmanager.")
      (license (license:non-copyleft "https://developer.android.com/studio/terms")))))

(define android-sdk (compose-android-sdk))
(define android-sdk-with-emulator (compose-android-sdk #:emulator? #t))
