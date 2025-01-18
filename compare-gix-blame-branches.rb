#!/usr/bin/env ruby

def print_usage
  print "usage: ./compare-gix-blame-branches.rb <baseline_executable> <comparison_executable> <limit> <offset>\n"
end

if not ENV.include?("GIT_WORK_TREE")
  print "env variable GIT_WORK_TREE not set\n"

  exit 1
end

if ARGV.count != 4
  print_usage

  exit 1
end

BLAME_LINE_REGEX = /([0-9a-f]+) (\d+) (\d+) (.*)/

GIT_WORK_TREE = ENV["GIT_WORK_TREE"]
GIT_DIR = "#{GIT_WORK_TREE}/.git"

baseline_executable = ARGV.shift
comparison_executable = ARGV.shift
limit = ARGV.shift.to_i
offset = ARGV.shift.to_i

files = `env GIT_DIR=#{GIT_WORK_TREE}/.git git ls-files --format="%(path) %(eolinfo:index)"`.lines(chomp: true)

print "#{files.size} files to run blame for, filtering out non-text files\n"

filenames = files.filter_map do |file|
  filename, attr = file.split(/\s+/)

  if attr.nil? or not attr.include?("-text")
    filename
  end
end

print "#{filenames.size} files to run blame for, limit #{limit}, offset #{offset}\n"
print "comparing blames\n"

filenames.each_with_index.drop(offset).take(limit).each do |filename, file_number|
  print "#{file_number} #{filename}\n"

  absolute_filename = "#{GIT_WORK_TREE}/#{filename}"

  baseline_blamed_lines = `env GIT_DIR=#{GIT_DIR} #{baseline_executable} blame "#{absolute_filename}"`.lines(chomp: true)
  comparison_blamed_lines = `env GIT_DIR=#{GIT_DIR} #{comparison_executable} blame "#{absolute_filename}"`.lines(chomp: true)

  if baseline_blamed_lines.size != comparison_blamed_lines.size
    print "blames have different number of lines\n"

    next
  end

  baseline_blamed_lines.zip(comparison_blamed_lines).each_with_index do |(baseline_line, comparison_line), line_number|
    match = baseline_line.match(BLAME_LINE_REGEX)

    if match.nil?
      print "`#{baseline_line}` does not look like a `gix blame` line\n"

      next
    end

    baseline_hash = match[1]

    match = comparison_line.match(BLAME_LINE_REGEX)

    if match.nil?
      print "`#{comparison_line}` does not look like a `gix blame` line\n"

      next
    end

    comparison_hash = match[1]

    if baseline_hash != comparison_hash
      line = match[4]

      print "hashes don't match for line #{line_number}: #{line}\n"
      print "baseline blamed #{baseline_hash} while comparison blamed #{comparison_hash}\n\n"
    end
  end
end
