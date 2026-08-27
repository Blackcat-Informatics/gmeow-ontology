#!/usr/bin/env perl
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

use strict;
use warnings;

my $producer_pattern = shift @ARGV;
my $repository_bound_pattern = shift @ARGV;
my $refresh_pattern = shift @ARGV;
my $producer_cli_pattern = shift @ARGV;
die "usage: $0 PRODUCER_REGEX REPOSITORY_BOUND_REGEX REFRESH_REGEX CLI_REGEX FILE...\n"
    unless defined $producer_pattern
        && defined $repository_bound_pattern
        && defined $refresh_pattern
        && defined $producer_cli_pattern;

my $producer_re = qr/$producer_pattern/;
my $repository_bound_re = qr/$repository_bound_pattern/;
my $refresh_re = qr/$refresh_pattern/;
my $producer_cli_re = qr/$producer_cli_pattern/;

sub rustdoc_lines {
    my ($path, $lines) = @_;
    return map { [$_ + 1, $lines->[$_]] } 0 .. $#$lines if $path =~ /\.md\z/;

    my @docs;
    my $in_block = 0;
    for my $index (0 .. $#$lines) {
        my $line = $lines->[$index];
        my $doc;
        if ($in_block) {
            $doc = $line;
            $doc =~ s/^\s*\*\s?//;
            if ($doc =~ s{\*/.*\z}{}) {
                $in_block = 0;
            }
        } elsif ($line =~ /^\s*\/\*[*!](.*)\z/) {
            $doc = $1;
            if ($doc =~ s{\*/.*\z}{}) {
                $in_block = 0;
            } else {
                $in_block = 1;
            }
        } elsif ($line =~ /^\s*\/\/(?:\/|!)[ ]?(.*)\z/) {
            $doc = $1;
        } else {
            next;
        }
        push @docs, [$index + 1, $doc];
    }
    return @docs;
}

sub rust_fence {
    my ($info) = @_;
    $info =~ s/^\s+|\s+$//g;
    return 1 if $info eq '';
    return $info =~ /(?:^|[,\s])(?:rust|no_run|should_panic|compile_fail|ignore|edition20\d\d)(?:$|[,\s])/;
}

for my $path (@ARGV) {
    open my $handle, '<', $path or die "read $path: $!\n";
    my @source = <$handle>;
    chomp @source;
    my @docs = rustdoc_lines($path, \@source);
    my @blocks;
    my ($fence_char, $fence_length, $is_rust);
    my @block;

    for my $entry (@docs) {
        my ($line_number, $line) = @$entry;
        if (!defined $fence_char) {
            next unless $line =~ /^\s*(`{3,}|~{3,})(.*)\z/;
            my $fence = $1;
            $fence_char = substr($fence, 0, 1);
            $fence_length = length $fence;
            $is_rust = rust_fence($2);
            @block = ();
            next;
        }

        if ($line =~ /^\s*\Q$fence_char\E{$fence_length,}\s*\z/) {
            push @blocks, [@block] if $is_rust;
            undef $fence_char;
            undef $fence_length;
            undef $is_rust;
            @block = ();
            next;
        }
        push @block, [$line_number, $line] if $is_rust;
    }

    for my $block (@blocks) {
        for my $index (0 .. $#$block) {
            my ($line_number, $line) = @{$block->[$index]};
            my $refresh = $line =~ $refresh_re;
            my $producer = $line =~ $producer_re || $line =~ $producer_cli_re;
            next unless $refresh || $producer;
            my $first = $index > 3 ? $index - 3 : 0;
            my $last = $index + 3 < $#$block ? $index + 3 : $#$block;
            my $synthetic = !$refresh
                && $line !~ $repository_bound_re
                && grep { $_->[1] =~ /gmeow-test-input: synthetic-only/ }
                    @{$block}[$first .. $last];
            next if $synthetic;
            print "$path:$line_number:$line\n";
        }
    }
}
