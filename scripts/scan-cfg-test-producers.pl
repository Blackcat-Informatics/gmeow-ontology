#!/usr/bin/env perl
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

use strict;
use warnings;

my $producer_pattern = shift @ARGV;
my $repository_bound_pattern = shift @ARGV;
my $refresh_pattern = shift @ARGV;
die "usage: $0 PRODUCER_REGEX REPOSITORY_BOUND_REGEX REFRESH_REGEX FILE...\n"
    unless defined $producer_pattern
        && defined $repository_bound_pattern
        && defined $refresh_pattern;
my $producer_re = qr/$producer_pattern/;
my $repository_bound_re = qr/$repository_bound_pattern/;
my $refresh_re = qr/$refresh_pattern/;

sub cfg_test_view {
    my ($source) = @_;
    my $code = $source;
    my $length = length $code;

    # Blank comments and literals in one regex pass before balancing item braces. The
    # recursive block-comment arm preserves Rust's nested-comment semantics; replacement
    # keeps byte and line offsets stable. Running the tokenization in Perl's regex engine
    # avoids a character-at-a-time interpreter loop over multi-megabyte source files.
    my $non_code = qr{
        (?<BLOCK> /\* (?: [^*/]+ | / (?! \*) | \* (?! /) | (?&BLOCK) )* \*/ )
      | // [^\n]*
      | (?:br|r) (?<HASH> \#*) " .*? " \k<HASH>
      | " (?: \\. | [^"\\] )* "
      | ' (?: \\. | [^'\\\n] ) '
    }xs;
    $code =~ s{$non_code}{
        my $span = $&;
        $span =~ tr/\n/ /c;
        $span;
    }gex;

    my $masked = $source;
    $masked =~ s/[^\n]/ /g;
    pos($code) = 0;
    while ($code =~ /\#\s*\[\s*cfg\s*\([^\]]*\btest\b[^\]]*\)\s*\]/g) {
        my $start = $-[0];
        my $cursor = $+[0];
        my $search_resume = $cursor;

        while (1) {
            pos($code) = $cursor;
            last unless $code =~ /\G\s*\#\s*\[[^\]]*\]/gcs;
            $cursor = pos($code);
        }
        $cursor++ while $cursor < $length && substr($code, $cursor, 1) =~ /\s/;

        my $brace = index($code, '{', $cursor);
        my $semicolon = index($code, ';', $cursor);
        my $open = $brace < 0
            ? $semicolon
            : $semicolon < 0
                ? $brace
                : $brace < $semicolon ? $brace : $semicolon;
        last if $open < 0;
        my $delimiter = substr($code, $open, 1);
        my $end;
        if ($delimiter eq ';') {
            $end = $open + 1;
        } else {
            my $depth = 1;
            $end = $open + 1;
            while ($end < $length && $depth) {
                my $next_open = index($code, '{', $end);
                my $next_close = index($code, '}', $end);
                last if $next_close < 0;
                if ($next_open >= 0 && $next_open < $next_close) {
                    $depth++;
                    $end = $next_open + 1;
                } else {
                    $depth--;
                    $end = $next_close + 1;
                }
            }
        }
        substr($masked, $start, $end - $start, substr($source, $start, $end - $start));
        pos($code) = $search_resume;
    }
    return $masked;
}

for my $path (@ARGV) {
    open my $handle, '<', $path or die "read $path: $!\n";
    local $/;
    my $source = <$handle>;
    my $view = cfg_test_view($source);
    my @lines = split /\n/, $view, -1;
    for my $index (0 .. $#lines) {
        my $refresh = $lines[$index] =~ $refresh_re;
        next unless $refresh || $lines[$index] =~ $producer_re;
        my $trimmed = $lines[$index];
        $trimmed =~ s/^\s*//;
        next if substr($trimmed, 0, 2) eq '//';
        my $first = $index > 3 ? $index - 3 : 0;
        my $last = $index + 3 < $#lines ? $index + 3 : $#lines;
        my $synthetic = !$refresh
            && $lines[$index] !~ $repository_bound_re
            && grep /gmeow-test-input: synthetic-only/, @lines[$first .. $last];
        next if $synthetic;
        print "$path:", $index + 1, ":$lines[$index]\n";
    }
}
