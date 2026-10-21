# Publishing to the AUR

`PKGBUILD` here is the template for the `cubetimer` AUR package. It builds the tagged
GitHub release, so tag first.

## First release

1. Push the repo and tag the version that matches `Cargo.toml`:
   ```sh
   git tag -a v0.2.0 -m "cubetimer 0.2.0"
   git push origin main --tags
   ```
2. Create an account at <https://aur.archlinux.org> and add your SSH public key to it.
3. Clone the (empty) AUR repo and copy the PKGBUILD in:
   ```sh
   git clone ssh://aur@aur.archlinux.org/cubetimer.git aur-cubetimer
   cp packaging/aur/PKGBUILD aur-cubetimer/ && cd aur-cubetimer
   updpkgsums                              # replaces SKIP with the tarball's sha256
   makepkg -f                              # build + run the tests; try: pacman -U cubetimer-*.pkg.tar.zst
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD .SRCINFO
   git commit -m "Initial import: cubetimer 0.2.0"
   git push origin master                  # the AUR uses `master`
   ```
   For a clean-chroot build (what the AUR guidelines recommend), install `devtools` and run
   `extra-x86_64-build` instead of `makepkg`.

## Each new release

Bump `version` in `Cargo.toml`, tag `vX.Y.Z`, then in the AUR repo set `pkgver=X.Y.Z`,
`pkgrel=1`, run `updpkgsums`, regenerate `.SRCINFO`, commit and push. Bump only `pkgrel`
when you change the packaging but not the program.

## Official repositories

The official `extra` repository takes packages through a Trusted User, usually only once
an AUR package has some votes and users. Publish to the AUR first.
