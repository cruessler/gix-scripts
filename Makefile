# This `Makefile` was heavily inspired by
# https://github.com/GitoxideLabs/gitoxide/blob/249bf9a2add29caa339c5f9783dd63f87a718c6e/Makefile

always:

gitoxide_repo = tests/fixtures/repos/gitoxide.git
$(gitoxide_repo):
	mkdir -p $@
	cd $@ && git clone https://github.com/GitoxideLabs/gitoxide .

gix := tests/fixtures/repos/gitoxide.git/target/release/gix
$(gix): $(gitoxide_repo)
	# Build `gix`.
	cd $(gitoxide_repo) && cargo build --release --no-default-features --features small

release:
	# Build `gix-scripts`.
	cargo build --release

prepare:
	$(MAKE) -j2 $(gix) release

# TODO
# Once it is confirmed everything is working as intended, start increasing
# `--take`, and eventually remove it.
compare: prepare
	./target/release/gix-scripts --git-work-tree="$(gitoxide_repo)" --baseline-executable="/usr/bin/git" --comparison-executable="$(gix)" --take 8
