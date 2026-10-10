// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! House-site compatibility with kramdown-parser-gfm 1.1.0 and Rouge 3.26.
//! Code wrappers retain the production structure, but deliberately omit tokens.

use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;

use crate::math::html_escape;

pub(super) fn render(source: &str, options: Options) -> String {
    let mut events: Vec<_> = Parser::new_ext(source, options).collect();
    let mut counts = HashMap::<String, usize>::new();
    // Ruby's Unicode Word category keeps letters, marks, decimal/letter
    // numbers, connector punctuation and the two join-control characters.
    static NON_WORD: OnceLock<Regex> = OnceLock::new();
    let non_word = NON_WORD.get_or_init(|| {
        Regex::new(r"[^\p{L}\p{M}\p{Nd}\p{Nl}\p{Pc}\u{200c}\u{200d}\- \t]").unwrap()
    });
    for start in 0..events.len() {
        if !matches!(events[start], Event::Start(Tag::Heading { .. })) {
            continue;
        }
        let mut text = String::new();
        let mut image_depth = 0;
        for event in &events[start + 1..] {
            match event {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Start(Tag::Image { .. }) => image_depth += 1,
                Event::End(TagEnd::Image) => image_depth -= 1,
                Event::Text(value) | Event::Code(value) if image_depth == 0 => text.push_str(value),
                _ => {}
            }
        }
        let mut id = non_word
            .replace_all(&text.to_lowercase(), "")
            .replace([' ', '\t'], "-");
        let count = counts.entry(id.clone()).or_default();
        if *count > 0 {
            id.push_str(&format!("-{count}"));
        }
        *count += 1;
        if let Event::Start(Tag::Heading { id: target, .. }) = &mut events[start] {
            *target = if id.is_empty() { None } else { Some(id.into()) };
        }
    }

    let mut language = None;
    let mut code = String::new();
    let mut in_code = false;
    let events = events.into_iter().filter_map(|event| match event {
        Event::Start(Tag::CodeBlock(kind)) => {
            in_code = true;
            language = match kind {
                CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_owned),
                CodeBlockKind::Indented => None,
            };
            code.clear();
            None
        }
        Event::Text(text) if in_code => {
            code.push_str(&text);
            None
        }
        Event::End(TagEnd::CodeBlock) => {
            in_code = false;
            Some(Event::Html(code_block(&code, language.as_deref()).into()))
        }
        other => Some(other),
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

fn code_block(code: &str, language: Option<&str>) -> String {
    let lang = language.unwrap_or("plaintext");
    // Escape text as kramdown does (quotes are literal in text nodes).
    let text = code
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let escaped_lang = html_escape(lang);
    if ROUGE_LANGUAGES
        .split_ascii_whitespace()
        .any(|known| known == lang)
    {
        format!("<div class=\"language-{escaped_lang} highlighter-rouge\"><div class=\"highlight\"><pre class=\"highlight\"><code>{text}</code></pre></div></div>\n")
    } else {
        let text = if text.ends_with('\n') {
            text
        } else {
            format!("{text}\n")
        };
        format!("<pre><code class=\"language-{escaped_lang}\">{text}</code></pre>\n")
    }
}

// Lexer aliases from the production bundle's Rouge 3.26.0. This only selects
// wrapper shape; no lexer runs, and Mermaid remains an ordinary code block.
const ROUGE_LANGUAGES: &str = "Dockerfile HAML LaTeX OCL R S TeX abap actionscript ada apache apex apib apiblueprint applescript armasm as as3 aug augeas awk bash bat batch batchfile bbcbasic behat bib bibtex biml bpf brainfuck brightscript brs bs bsdmake bsl c c# c++ ceylon cfc cfscript cl clean clj cljs clojure cmake cmhg cmm coffee coffee-script coffeescript common-lisp common_lisp conf config configuration console coq cpp cr crystal cs csharp css csvs cucumber cuda cypher cython d dart datastudio diff digdag django dlang docker dockerfile dosbatch dot e-mail ecl eex eiffel elisp elixir elm emacs-lisp email eml epp eps erb erl erlang eruby esc escape ex exs factor fea ff fortran freefem fsharp gd gdscript ghc-cmm ghc-core gherkin glsl gnumake go golang gradle graphql groovy hack haml handlebars haskell haxe hbs hcl hh hlsl hocon hql hs html http hx hy hylang idlang igorpro ini io irb irb_output isbl j janet java javascript jdn jinja jl js jsl json json-doc jsonc jsonnet jsp jsx julia kdb+ kotlin ksh lasso lassoscript latex leex lhaskell lhs liquid lisp litcoffee literate_coffeescript literate_haskell lithaskell livescript llvm ls lua lustre lutin m m68k magik make makefile markdown mason mathematica matlab md mf microsoftshell minizinc mkd ml moon moonscript mosel msgtrans msshell mustache mxml nasm nes nesasm nginx nim nimrod nix nixos obj-c obj-c++ obj-cpp obj_c obj_c++ obj_cpp objc objc++ objcpp objective_c objective_cpp objectivec objectivec++ objectivecpp ocaml ocl openedge opentype opentype_feature_file opentypefeature pascal patch perl php php3 php4 php5 pl plaintext plist pony posh postscr postscript powershell pp praat prolog prometheus properties proto protobuf pry ps puppet py pyrex python pyx q qml r racket rb react realbasic reasonml rego rescript rhtml robot robot-framework robot_framework rs rs,ignore rs,no_run rs,should_panic ruby rust rust,ignore rust,no_run rust,should_panic s sas sass scala scheme scss sed sh shell shell-session shell_session sieve slice slim smalltalk smarty sml solidity sparql sqf sql squeak ssh st supercollider swift systemd tap tcl terminal terraform tex text tf toml ts tsx ttcn3 tulip turtle twig typescript udiff unit-file vala varnish varnishconf vb vcl velocity verilog vhdl vim viml vimscript visualbasic vue vuejs winbatch wl wollok xml xojo xpath xquery yaml yang yml zig zir zsh";
