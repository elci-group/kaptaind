.PHONY: install-man

MANDIR ?= /usr/local/share/man

install-man:
	@mkdir -p $(MANDIR)/man1
	@if command -v pandoc >/dev/null 2>&1; then \
		echo "Rendering man pages with pandoc..."; \
		pandoc man/kaptaind.1.md -s -t man -o $(MANDIR)/man1/kaptaind.1; \
		pandoc man/kaptaind-cli.1.md -s -t man -o $(MANDIR)/man1/kaptaind-cli.1; \
	else \
		echo "pandoc not found; installing Markdown sources as reference..."; \
		cp man/kaptaind.1.md $(MANDIR)/man1/kaptaind.1.md; \
		cp man/kaptaind-cli.1.md $(MANDIR)/man1/kaptaind-cli.1.md; \
	fi
	@echo "Man pages installed to $(MANDIR)/man1"
