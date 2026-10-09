#!/usr/bin/env ruby
# frozen_string_literal: true
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Shawn Hartsock and contributors

require "tmpdir"
require "fileutils"
require "json"
require "stringio"

# Deliberately no arbitrary corpus mode, directory discovery, or symlink inputs.
NAMES = %w[autolink-backslash comment-definition fence-attributes frontmatter-crlf
           hard-break heading-id indented-code liquid-expression liquid-tag probe
           reference-link sample table-delimiter].freeze
expected = File.expand_path("../scrybe-app/test/golden", __dir__)
begin
  valid = ARGV.length == 1 && File.realpath(ARGV[0]) == expected &&
          File.realpath(expected) == expected && NAMES.all? do |name|
            ["#{name}.md", "#{name}.out.md"].all? do |file|
              path = File.join(expected, file)
              File.file?(path) && !File.symlink?(path)
            end
          end
rescue SystemCallError
  valid = false
end
unless valid
  warn "Refused: supply only this checkout's scrybe-app/test/golden directory with regular synthetic pairs."
  exit 2
end

gem "jekyll", "3.10.0"
gem "kramdown", "2.4.0"
require "jekyll"
require "kramdown-parser-gfm"

CONFIG = {
  "markdown" => "kramdown",
  "kramdown" => { "input" => "GFM", "hard_wrap" => false },
  "highlighter" => "rouge",
  "safe" => true,
  "plugins" => [],
  "theme" => nil,
  "quiet" => true,
  "incremental" => false,
  "include" => ["_posts"],
  "defaults" => [{ "scope" => { "path" => "" }, "values" => { "layout" => nil } }]
}.freeze

# Prefix/suffix trimming gives the smallest single differing span, with a short
# context window so an empty span (an insertion/deletion) remains interpretable.
def difference(left, right)
  prefix = 0
  prefix += 1 while prefix < [left.length, right.length].min && left[prefix] == right[prefix]
  suffix = 0
  suffix += 1 while suffix < [left.length, right.length].min - prefix && left[-suffix - 1] == right[-suffix - 1]
  {
    "offset" => prefix,
    "input" => left[prefix, left.length - prefix - suffix],
    "pinned" => right[prefix, right.length - prefix - suffix],
    "before" => left[[prefix - 35, 0].max...prefix],
    "after" => suffix.zero? ? "" : left[left.length - suffix, [suffix, 35].min]
  }
end

def render(source, directory)
  input = File.join(directory, "source")
  output = File.join(directory, "output")
  FileUtils.mkdir_p([File.join(input, "_data"), File.join(input, "_posts")])
  File.write(File.join(input, "_data/nav.yml"), "- title: Synthetic navigation title\n")
  # Static files deliberately use exactly the link targets in the goldens.
  %w[example x].each { |name| File.write(File.join(input, "_posts/#{name}.md"), "Synthetic link target.\n") }
  # Preserve existing front matter verbatim; otherwise enable Jekyll processing
  # with an empty block. The site default supplies layout: null in either case.
  source = "---\n---\n#{source}" unless Jekyll::Document::YAML_FRONT_MATTER_REGEXP.match?(source)
  File.binwrite(File.join(input, "page.md"), source)
  config = Jekyll.configuration(CONFIG.merge("source" => input, "destination" => output, "config" => []))
  Jekyll::Site.new(config).process
  { "html" => File.binread(File.join(output, "page.html")).force_encoding(Encoding::UTF_8) }
rescue StandardError => error
  # Never emit temporary paths or a machine-dependent backtrace.
  { "error" => "#{error.class}: #{error.message.lines.first.to_s.strip.gsub(directory, '<temporary-site>')}" }
end

results = []
# A block-form temporary directory is removed even if a build raises.
Dir.mktmpdir("scrybe-synthetic-jekyll-") do |root|
  NAMES.each do |name|
    sides = ["#{name}.md", "#{name}.out.md"].each_with_index.map do |file, index|
      # Jekyll logs exceptions before raising. Capture these to avoid leaking
      # temporary paths; the sanitized exception is reported in the result.
      previous_stdout, previous_stderr = $stdout, $stderr
      begin
        $stdout = StringIO.new
        $stderr = StringIO.new
        Jekyll.logger = Logger.new($stdout)
        render(File.binread(File.join(expected, file)).force_encoding(Encoding::UTF_8), File.join(root, "#{name}-#{index}"))
      ensure
        $stdout, $stderr = previous_stdout, previous_stderr
      end
    end
    row = { "pair" => name }
    if sides.any? { |side| side.key?("error") }
      row.merge!("exact_equal" => nil, "whitespace_equal" => nil,
                 "input_error" => sides[0]["error"], "pinned_error" => sides[1]["error"],
                 "note" => "Build error; HTML equality is not established")
      row["input_html"] = sides[0]["html"] if sides[0].key?("html")
      row["pinned_html"] = sides[1]["html"] if sides[1].key?("html")
    else
      left, right = sides.map { |side| side.fetch("html") }
      normalized = [left, right].map { |html| html.gsub(/>\s+</, "><") }
      row.merge!("exact_equal" => left == right, "whitespace_equal" => normalized[0] == normalized[1],
                 "note" => left == right ? "Same HTML" : "HTML differs")
      row["liquid_html"] = left if name == "liquid-expression"
      row["difference"] = difference(left, right) unless left == right
      row["whitespace_difference"] = difference(*normalized) unless normalized[0] == normalized[1]
    end
    results << row
  end
end
puts JSON.pretty_generate({
  "environment" => { "ruby" => RUBY_VERSION, "platform" => RUBY_PLATFORM,
                     "gems" => %w[jekyll kramdown kramdown-parser-gfm liquid rouge].to_h { |name| [name, Gem.loaded_specs.fetch(name).version.to_s] } },
  "config" => CONFIG,
  "results" => results
})
# Nonzero means the comparison does not establish equivalent rendered HTML.
exit(results.all? { |row| row["exact_equal"] && row["whitespace_equal"] } ? 0 : 1)
