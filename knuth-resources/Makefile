CTANGLE ?= ctangle
CC ?= cc
CFLAGS ?= -O2 -std=gnu89

.PHONY: all clean

all: dlx1

dlx1: dlx1.c gb_flip.c gb_flip.h
	$(CC) $(CPPFLAGS) $(CFLAGS) $(LDFLAGS) -o $@ dlx1.c gb_flip.c $(LDLIBS)

dlx1.c: dlx1.w
	$(CTANGLE) $<

# One ctangle invocation generates all three files.
gb_flip.c gb_flip.h test_flip.c &: gb_flip.w boilerplate.w
	$(CTANGLE) $<

clean:
	rm -f dlx1 dlx1.c gb_flip.c gb_flip.h test_flip.c
