#!/usr/bin/env perl
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# A bounded lexical flow seal, not a Rust compiler or complete interprocedural proof.
# Follow literal slices/dsl/imports/conformance paths through bindings, constants, reads/includes,
# and statically named same-file helper arguments/returns into parse/lower/build calls
# or command arguments. Authenticated bundle data is already produced: grading it
# is allowed, but another lowering/reasoning pass is corpus production in a test.
# A child process cannot launder an authored corpus input.
# Only test-reachable functions contribute edges. No macro expansion, cross-file name
# resolution, trait dispatch or closure-parameter analysis is claimed. Existing named
# producer seals remain independent. Synthetic markers cannot erase authored origin.

use strict;
use warnings;

sub lex {
    my ($source, $path) = @_;
    my @tokens;
    my $comment = qr{(?<BLOCK>/\*(?:[^*/]+|/(?!\*)|\*(?!/)|(?&BLOCK))*\*/)|//[^\n]*};
    pos($source) = 0;
    while (pos($source) < length $source) {
        next if $source =~ /\G(?:\s+|$comment)/gc;
        my $offset = pos($source);
        if ($source =~ /\G(?:br|r)(\#*)"(.*?)"\1/gcs) {
            push @tokens, { text => $2, kind => 'string', offset => $offset };
        } elsif ($source =~ /\Gb?"((?:\\.|[^"\\])*)"/gcs) {
            push @tokens, { text => $1, kind => 'string', offset => $offset };
        } elsif ($source =~ /\Gb?'(?:\\(?:u\{[A-Fa-f0-9_]+\}|x[A-Fa-f0-9]{2}|.)|[^'\\\n])'/gc) {
            push @tokens, { text => '', kind => 'char', offset => $offset };
        } elsif ($source =~ /\G([A-Za-z_]\w*|::|->|=>|.)/gcs) {
            push @tokens, { text => $1, kind => 'code', offset => $offset };
        } else {
            die "$path: cannot tokenize byte $offset\n";
        }
    }
    my @stack;
    for my $i (0 .. $#tokens) {
        next unless $tokens[$i]{kind} eq 'code';
        my $text = $tokens[$i]{text};
        if ($text =~ /^[({\[]$/) {
            push @stack, $i;
        } elsif ($text =~ /^[)}\]]$/) {
            my $open = pop @stack;
            die "$path: unbalanced Rust delimiters\n" unless defined $open
                && index('({[', $tokens[$open]{text}) == index(')}]', $text);
            $tokens[$open]{close} = $i;
        }
    }
    die "$path: unclosed Rust delimiter\n" if @stack;
    return \@tokens;
}

sub text_at { return $_[0]{tokens}[$_[1]]{text} // ''; }
sub node {
    my ($state, @dependencies) = @_;
    push @{$state->{nodes}}, {
        deps => \@dependencies, path => 0, data => 0, artifact => 0,
        temporary => 0, repository => 0,
    };
    return $#{$state->{nodes}};
}
sub edge { my ($state, $to, @from) = @_; push @{$state->{nodes}[$to]{deps}}, @from; }

# Split parameter/argument lists without splitting nested tuples, calls or blocks.
sub parts {
    my ($state, $start, $end, $separator) = @_;
    my @parts;
    my $first = $start;
    for (my $i = $start; $i < $end; $i++) {
        if (defined $state->{tokens}[$i]{close}) {
            $i = $state->{tokens}[$i]{close};
        } elsif (text_at($state, $i) eq $separator) {
            push @parts, [$first, $i];
            $first = $i + 1;
        }
    }
    push @parts, [$first, $end] if $first < $end;
    return @parts;
}

sub local_function {
    my ($state, $context, $name) = @_;
    my @scope = @{$context->{scope}};
    my @name = split /::/, $name;
    if (@name > 1) {
        shift @name if $name[0] eq 'self';
        while (@name && $name[0] eq 'super') { shift @name; pop @scope; }
    }
    while (1) {
        my $key = join '::', @scope, @name;
        return $state->{functions}{$key} if exists $state->{functions}{$key};
        last unless @scope;
        pop @scope;
    }
    return;
}

sub binding {
    my ($state, $context, $name) = @_;
    return $context->{vars}{$name} if exists $context->{vars}{$name};
    my @scope = @{$context->{scope}};
    while (1) {
        my $key = join '::', @scope, $name;
        if (my $constant = $state->{constants}{$key}) {
            if (!$constant->{evaluated}++) {
                edge($state, $constant->{node}, expression($state, $constant,
                    $constant->{start}, $constant->{end}));
            }
            return $constant->{node};
        }
        last unless @scope;
        pop @scope;
    }
    return $context->{vars}{$name} = node($state);
}

sub record_transform {
    my ($state, $result, $offset, $name) = @_;
    my $kind =
        $name =~ /(?:^|::)(?:to_grammar|with_owl_rdfs_projection|shacl_reader_view|lower(?:_\w+)?|compile(?:_\w+)?|prepare_reasoning_input|reason_(?:all(?:_budgeted|_with_data)?|program(?:_closure_dataset)?|closure(?:_axioms|_dataset)?)|dl_consistency|foundation_evaluate)$/
        ? 'semantic'
        : $name =~ /(?:^|::)(?:dataset_from_(?:bytes|str)|parse(?:_\w+)?|from_dataset|build(?:_\w+)?|write)$/
        ? 'decode'
        : undef;
    if (defined $kind) {
        push @{$state->{sinks}}, [$result, $offset, $name, $kind];
    }
}

sub record_corpus_reader {
    my ($state, $result, $name) = @_;
    # A selected derived artifact may be decoded and graded, but it remains an
    # authenticated corpus product and cannot become input to another semantic
    # producer. Native repository/source selections are stronger: even another
    # parse/lowering step would reconstruct the corpus.
    $state->{nodes}[$result]{data} = 1
        if $name =~ /(?:^|::)load_authenticated_(?:repository_bundle|source_bytes)$/;
    $state->{nodes}[$result]{artifact} = 1
        if $name =~ /(?:^|::)(?:authenticated_artifacts?|load_authenticated_(?:corpus_artifact|corpus_archive))$/;
}

# Give each local invocation its own argument/return flow. Reuse an ancestor
# instance only on a recursive back-edge, keeping expansion finite without mixing
# a corpus call into an unrelated synthetic call to the same helper.
sub invocation {
    my ($state, $function, $ancestry) = @_;
    my $key = $function->{key};
    return $state->{instances}[$ancestry->{$key}] if exists $ancestry->{$key};
    my $instance = {
        scope => $function->{scope}, key => $key,
        start => $function->{start}, end => $function->{end},
        vars => {}, params => [], result => node($state),
        ancestry => { %$ancestry },
    };
    for my $name (@{$function->{param_names}}) {
        my $parameter = node($state);
        $instance->{vars}{$name} = $parameter;
        push @{$instance->{params}}, $parameter;
    }
    $instance->{ancestry}{$key} = scalar @{$state->{instances}};
    push @{$state->{instances}}, $instance;
    push @{$state->{queue}}, $instance;
    return $instance;
}

sub call {
    my ($state, $context, $name, $offset, $method, @arguments) = @_;
    if (my $function = $method ? undef : local_function($state, $context, $name)) {
        # Moving a test beside its parser cannot erase the parsing boundary.
        # Follow local returns as before, and independently record authored bytes
        # admitted to a named transformation even when its body is available.
        record_transform($state, node($state, @arguments), $offset, $name);
        my $instance = invocation($state, $function, $context->{ancestry} // {});
        for my $i (0 .. $#arguments) {
            edge($state, $instance->{params}[$i], $arguments[$i])
                if defined $instance->{params}[$i];
        }
        record_corpus_reader($state, $instance->{result}, $name);
        return $instance->{result};
    }
    my $result = node($state, @arguments);
    record_corpus_reader($state, $result, $name);
    $state->{nodes}[$result]{temporary} = 1
        if $name =~ /(?:^|::)(?:tempdir|temp_dir)$/ || $name =~ /(?:^|::)TempDir::new$/;
    if ($name =~ /(?:^|::)(?:read|read_to_string|read_to_end|open|include_str|include_bytes)$/) {
        $state->{nodes}[$result]{read} = 1;
        $state->{nodes}[$result]{repository} = 1 if $name =~ /(?:^|::)include_(?:str|bytes)$/;
    }
    record_transform($state, $result, $offset, $name);
    if ($name =~ /(?:^|::)copy$/ && @arguments) {
        my $source = node($state, $arguments[0]);
        $state->{nodes}[$source]{read} = 1;
        push @{$state->{sinks}}, [$source, $offset, $name];
    }
    if ($method && $name =~ /^args?$/) {
        # The child process may parse or lower the supplied file. Treat passing
        # authored inputs as a read boundary, including path-returning helpers.
        $state->{nodes}[$result]{read} = 1;
        push @{$state->{sinks}}, [$result, $offset, $name];
    }
    return $result;
}

# Expressions conservatively combine their operands. Assignments retain per-binding
# flow, so reading an authored path elsewhere in a test does not taint a tiny literal.
sub expression {
    my ($state, $context, $start, $end) = @_;
    my @values;
    for (my $i = $start; $i < $end; $i++) {
        my $token = $state->{tokens}[$i];
        my $text = $token->{text};
        if ($token->{kind} eq 'string') {
            my $value = node($state);
            $state->{nodes}[$value]{path} = 1
                if $text !~ m{://} && $text =~ m{(?:^|/)(?:slices|dsl|imports|conformance)(?:/|$)};
            $state->{nodes}[$value]{repository} = 1 if $text eq 'CARGO_MANIFEST_DIR';
            push @values, $value;
        } elsif ($token->{kind} ne 'code') {
            next;
        } elsif ($text eq 'for') {
            my $in = $i + 1;
            $in++ while $in < $end && text_at($state, $in) !~ /^(?:in|\{)$/;
            if ($in < $end && text_at($state, $in) eq 'in') {
                my $open = $in + 1;
                while ($open < $end && text_at($state, $open) ne '{') {
                    $open = $state->{tokens}[$open]{close}
                        if defined $state->{tokens}[$open]{close};
                    $open++;
                }
                if ($open < $end && defined $state->{tokens}[$open]{close}) {
                    my $iterated = expression($state, $context, $in + 1, $open);
                    my %nested = (%$context, vars => { %{$context->{vars}} });
                    # Each destructured component conservatively inherits the
                    # iterator's authored origin; no positional narrowing.
                    for my $at ($i + 1 .. $in - 1) {
                        my $name = text_at($state, $at);
                        $nested{vars}{$name} = node($state, $iterated)
                            if $name =~ /^[A-Za-z_]\w*$/ && $name !~ /^(?:mut|ref|_)$/;
                    }
                    push @values, body($state, \%nested, $open + 1, $state->{tokens}[$open]{close});
                    $i = $state->{tokens}[$open]{close};
                }
            }
        } elsif ($text eq '{') {
            my %nested = (%$context, vars => { %{$context->{vars}} });
            push @values, body($state, \%nested, $i + 1, $token->{close});
            $i = $token->{close};
        } elsif (defined $token->{close}) {
            push @values, expression($state, $context, $i + 1, $token->{close});
            $i = $token->{close};
        } elsif ($text =~ /^[A-Za-z_]\w*$/) {
            my $name = $text;
            my $last = $i;
            while (text_at($state, $last + 1) eq '::'
                && text_at($state, $last + 2) =~ /^[A-Za-z_]\w*$/) {
                $name .= '::' . text_at($state, $last + 2);
                $last += 2;
            }
            my $open = $last + 1;
            $open++ if text_at($state, $open) eq '!';
            if (text_at($state, $open) eq '(' && defined $state->{tokens}[$open]{close}) {
                my $close = $state->{tokens}[$open]{close};
                my @spans = parts($state, $open + 1, $close, ',');
                my @arguments = map { expression($state, $context, @$_) } @spans;
                # A method's receiver carries source origin too (source.as_bytes(),
                # File::open(path).read_to_string(...), grammar_source.parse(), ...).
                my $method = $i > $start && text_at($state, $i - 1) eq '.';
                unshift @arguments, node($state, @values) if $method;
                my $result = call($state, $context, $name, $token->{offset}, $method, @arguments);
                if ($method && $name =~ /^read_to_(?:string|end)$/) {
                    for my $span (@spans) {
                        my ($first, $last) = @$span;
                        if ($last == $first + 3 && text_at($state, $first) eq '&'
                            && text_at($state, $first + 1) eq 'mut') {
                            edge($state, binding($state, $context, text_at($state, $first + 2)), $result);
                        }
                    }
                }
                push @values, $result;
                $i = $close;
            } else {
                push @values, binding($state, $context, $name);
                $i = $last;
            }
        }
    }
    return node($state, @values);
}

sub body {
    my ($state, $context, $start, $end) = @_;
    my $tail = node($state);
    for my $part (parts($state, $start, $end, ';')) {
        my ($first, $last) = @$part;
        next if $first == $last;
        # Function items are evaluated only when reachable, never as expressions.
        next if text_at($state, $first) eq 'fn';
        my $equal;
        for (my $i = $first; $i < $last; $i++) {
            if (defined $state->{tokens}[$i]{close}) { $i = $state->{tokens}[$i]{close}; }
            elsif (text_at($state, $i) eq '=' && text_at($state, $i + 1) ne '='
                && text_at($state, $i - 1) !~ /^[=!<>]$/) { $equal = $i; last; }
        }
        if (defined $equal && text_at($state, $first) =~ /^(?:let|const|static)$/) {
            my $name_at = $first + 1;
            $name_at++ if text_at($state, $name_at) eq 'mut';
            my $name = text_at($state, $name_at);
            my $value = expression($state, $context, $equal + 1, $last);
            if ($name eq '(' && defined $state->{tokens}[$name_at]{close}) {
                for my $at ($name_at + 1 .. $state->{tokens}[$name_at]{close} - 1) {
                    my $binding = text_at($state, $at);
                    $context->{vars}{$binding} = node($state, $value)
                        if $binding =~ /^[A-Za-z_]\w*$/ && $binding !~ /^(?:mut|ref|_)$/;
                }
            } else {
                $context->{vars}{$name} = node($state, $value);
            }
            $tail = node($state);
        } elsif (defined $equal && $equal == $first + 1
            && text_at($state, $first) =~ /^[A-Za-z_]\w*$/) {
            my $value = expression($state, $context, $equal + 1, $last);
            edge($state, binding($state, $context, text_at($state, $first)), $value);
            $tail = node($state);
        } else {
            $tail = expression($state, $context, $first, $last);
            edge($state, $context->{result}, $tail)
                if text_at($state, $first) eq 'return' && defined $context->{result};
        }
    }
    return $tail;
}

sub scan {
    my ($path) = @_;
    open my $handle, '<:encoding(UTF-8)', $path or die "read $path: $!\n";
    local $/;
    my $source = <$handle>;
    close $handle or die "close $path: $!\n";
    $source = '' unless defined $source;
    my $test_file = $path =~ m{(?:^|/)(?:tests|corpus_tests)/|(?:^|/)[^/]*tests\.rs$|(?:^|/)build\.rs$};
    # This only rejects files with no possible test root; it never selects source
    # by known producer spellings. Comments can add work here, not hide a root.
    return unless $test_file || $source =~ /\#\s*\[\s*(?:cfg\b[^\]]*\btest\b|(?:\w+\s*::\s*)*test\b)/s;
    my $tokens = lex($source, $path);
    my $state = { tokens => $tokens, nodes => [], functions => {}, constants => {}, queue => [], instances => [], sinks => [] };
    my (@modules, @tests, @roots);
    for my $i (0 .. $#$tokens) {
        my $text = text_at($state, $i);
        if ($text eq 'mod' && text_at($state, $i + 2) eq '{') {
            push @modules, [$i + 2, $tokens->[$i + 2]{close}, text_at($state, $i + 1)];
        }
        next unless $text eq '#' && text_at($state, $i + 1) eq '[';
        my $close = $tokens->[$i + 1]{close};
        my $attribute = join '', map { $_->{text} } @$tokens[$i + 2 .. $close - 1];
        next unless $attribute =~ /^(?:(?:\w+::)*test|cfg\(.*\btest\b.*\))$/;
        my $open = $close + 1;
        while ($open <= $#$tokens && text_at($state, $open) ne '{' && text_at($state, $open) ne ';') { $open++; }
        push @tests, [$i, $tokens->[$open]{close}] if $open <= $#$tokens && defined $tokens->[$open]{close};
    }
    for my $i (0 .. $#$tokens) {
        my $text = text_at($state, $i);
        next unless $text =~ /^(?:fn|const|static)$/;
        my @scope = map { $_->[2] } grep { $_->[0] < $i && $_->[1] > $i } @modules;
        my $name = text_at($state, $i + 1);
        next unless $name =~ /^[A-Za-z_]\w*$/;
        my $context = { scope => \@scope, vars => {}, node => node($state) };
        my $cursor = $i + 2;
        if ($text ne 'fn') {
            $cursor++ while $cursor <= $#$tokens && text_at($state, $cursor) !~ /^[=;{]$/;
            next unless text_at($state, $cursor) eq '=';
            $context->{start} = ++$cursor;
            while ($cursor <= $#$tokens && text_at($state, $cursor) ne ';') {
                $cursor = $tokens->[$cursor]{close} if defined $tokens->[$cursor]{close};
                $cursor++;
            }
            $context->{end} = $cursor;
            $state->{constants}{join '::', @scope, $name} = $context;
            next;
        }
        $cursor++ while $cursor <= $#$tokens && text_at($state, $cursor) !~ /^[({;]$/;
        next unless text_at($state, $cursor) eq '(';
        my $close = $tokens->[$cursor]{close};
        my @param_names;
        for my $part (parts($state, $cursor + 1, $close, ',')) {
            my ($first, $last) = @$part;
            $first++ while $first < $last && text_at($state, $first) =~ /^(?:&|mut|ref)$/;
            push @param_names, text_at($state, $first);
        }
        $cursor = $close + 1;
        $cursor++ while $cursor <= $#$tokens && text_at($state, $cursor) !~ /^[{;]$/;
        next unless text_at($state, $cursor) eq '{';
        @$context{qw(start end param_names key)} =
            ($cursor + 1, $tokens->[$cursor]{close}, \@param_names, join '::', @scope, $name);
        $state->{functions}{$context->{key}} = $context;
        if ($test_file || grep { $_->[0] <= $i && $_->[1] > $i } @tests) {
            push @roots, $context;
        }
    }
    invocation($state, $_, {}) for @roots;
    while (my $context = shift @{$state->{queue}}) {
        edge($state, $context->{result}, body($state, $context, $context->{start}, $context->{end}));
    }
    # First settle path origins. Fresh tempfile roots are synthetic, while an
    # include/CARGO_MANIFEST_DIR origin remains repository-owned. Authored bytes
    # copied or written into a temp directory are independently rejected above.
    # Keeping this phase separate avoids prematurely classifying a temporary read.
    my $changed = 1;
    while ($changed) {
        $changed = 0;
        for my $node (@{$state->{nodes}}) {
            for my $kind (qw(path temporary repository)) {
                my $before = $node->{$kind};
                for my $dependency (@{$node->{deps}}) {
                    $node->{$kind} ||= $state->{nodes}[$dependency]{$kind};
                }
                $changed ||= $before != $node->{$kind};
            }
        }
    }
    # A monotone fixed point supports mutually recursive local helpers without
    # executing them or depending on a guessed recursion/iteration budget.
    $changed = 1;
    while ($changed) {
        $changed = 0;
        for my $node (@{$state->{nodes}}) {
            my ($before_data, $before_artifact) = @{$node}{qw(data artifact)};
            for my $dependency (@{$node->{deps}}) {
                $node->{data} ||= $state->{nodes}[$dependency]{data};
                $node->{artifact} ||= $state->{nodes}[$dependency]{artifact};
            }
            $node->{data} ||= $node->{path} && (!$node->{temporary} || $node->{repository}) if $node->{read};
            $changed ||= $before_data != $node->{data}
                || $before_artifact != $node->{artifact};
        }
    }
    my %reported;
    for my $sink (@{$state->{sinks}}) {
        my ($value, $offset, $name, $kind) = @$sink;
        my $forbidden = $state->{nodes}[$value]{data}
            || (($kind // '') eq 'semantic' && $state->{nodes}[$value]{artifact});
        next unless $forbidden && !$reported{$offset}++;
        my $prefix = substr($source, 0, $offset);
        my $line = 1 + ($prefix =~ tr/\n/\n/);
        print "$path:$line: authored-source bytes reach $name through local test flow\n";
    }
}

die "usage: $0 RUST_FILE...\n" unless @ARGV;
scan($_) for @ARGV;
