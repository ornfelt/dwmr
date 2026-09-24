# dwmr - dynamic window manager (Rust port of dwm)
# See LICENSE file for copyright and license details.

VERSION = 6.8.0

# paths
PREFIX = /usr/local
MANPREFIX = ${PREFIX}/share/man

CARGO = cargo
BIN = target/release/dwmr
USERHOME = ${HOME}

# cargo/rustup toolchains are per user, so under `sudo make install` build
# (and find the config dir) as the invoking user, not as root
ifneq (${SUDO_USER},)
CARGO = sudo -u ${SUDO_USER} -H cargo
USERHOME = $(shell getent passwd ${SUDO_USER} | cut -d: -f6)
endif
CONFDIR = ${USERHOME}/.config/dwmr

all: ${BIN}

${BIN}: Cargo.toml src/*.rs
	${CARGO} build --release

test:
	${CARGO} test

clean:
	${CARGO} clean

install: all
	mkdir -p ${DESTDIR}${PREFIX}/bin
	cp -f ${BIN} ${DESTDIR}${PREFIX}/bin/dwmr
	chmod 755 ${DESTDIR}${PREFIX}/bin/dwmr
	mkdir -p ${DESTDIR}${MANPREFIX}/man1
	sed "s/VERSION/${VERSION}/g" < dwmr.1 > ${DESTDIR}${MANPREFIX}/man1/dwmr.1
	chmod 644 ${DESTDIR}${MANPREFIX}/man1/dwmr.1

# copy the default config to ~/.config/dwmr unless one is already there
install-config:
	mkdir -p ${CONFDIR}
	[ -e ${CONFDIR}/config.toml ] || cp config/config.toml ${CONFDIR}/config.toml

uninstall:
	rm -f ${DESTDIR}${PREFIX}/bin/dwmr\
		${DESTDIR}${MANPREFIX}/man1/dwmr.1

.PHONY: all test clean install install-config uninstall
