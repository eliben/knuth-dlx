CTANGLE ?= ctangle
CC ?= cc
CFLAGS ?= -O2 -std=gnu89

.PHONY: all clean

all: dlx1 dance

dlx1: dlx1.c gb_flip.c gb_flip.h
	$(CC) $(CPPFLAGS) $(CFLAGS) $(LDFLAGS) -o $@ dlx1.c gb_flip.c $(LDLIBS)

dlx1.c: dlx1.w
	$(CTANGLE) $<

dance: dance.c
	$(CC) $(CPPFLAGS) $(CFLAGS) $(LDFLAGS) -o $@ dance.c $(LDLIBS)

dance.c: dance.w
	$(CTANGLE) $<

# One ctangle invocation generates all three files.
gb_flip.c gb_flip.h test_flip.c &: gb_flip.w boilerplate.w
	$(CTANGLE) $<

clean:
	rm -f dlx1 dlx1.c gb_flip.c gb_flip.h test_flip.c dance.c dance
