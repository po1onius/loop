;; Reproduce the Guix package definitions used for this environment:
;; guix time-machine -C guix/channels.scm -- shell -L guix/modules -m guix/android.scm
(use-modules (guix channels))

(list
 (channel
  (name 'guix)
  (url "https://codeberg.org/guix/guix.git")
  (branch "master")
  (commit "06507e0ecffb8016b372d7a922698de431c3ff64")
  (introduction
   (make-channel-introduction
    "9edb3f66fd807b096b48283debdcddccfea34bad"
    (openpgp-fingerprint
     "BBB0 2DDF 2CEA F6A8 0D1D E643 A2A0 6DF2 A33A 54FA")))))
