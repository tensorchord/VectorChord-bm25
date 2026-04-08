export PG_CONFIG ?= pg_config
PKGLIBDIR := $(shell $(PG_CONFIG) --pkglibdir)
SHAREDIR := $(shell $(PG_CONFIG) --sharedir)
CP_R ?= cp -r
MKDIR_P ?= mkdir -p

.PHONY: all build clippy install uninstall

all: build

build:
	cargo run -p xtask -- build

clippy:
	cargo run -p xtask -- clippy

install:
	$(MKDIR_P) $(DESTDIR)$(PKGLIBDIR) && \
	$(CP_R) ./build/pkglibdir/. $(DESTDIR)$(PKGLIBDIR) && \
	$(MKDIR_P) $(DESTDIR)$(SHAREDIR) && \
	$(CP_R) ./build/sharedir/. $(DESTDIR)$(SHAREDIR)

uninstall:
	$(RM) $(wildcard $(DESTDIR)$(PKGLIBDIR)/vchord_bm25.*) && \
	$(RM) $(wildcard $(DESTDIR)$(SHAREDIR)/extension/vchord_bm25.*) && \
	$(RM) $(wildcard $(DESTDIR)$(SHAREDIR)/extension/vchord_bm25--*.sql)
