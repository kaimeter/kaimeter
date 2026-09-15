-- Build-time transformations for the whitepaper source.
--
-- Two jobs:
-- 1. the Markdown source carries its own title block for online reading; the
--    LaTeX template renders that information as a title page, so drop the
--    source block and promote the remaining headings by one level;
-- 2. Latin Modern (the only text font in the pinned image) cannot typeset a
--    handful of symbols used in the paper, so replace them with LaTeX
--    commands defined in the template. Code blocks are transliterated to
--    ASCII because verbatim environments cannot switch fonts.

local symbol_commands = {
  [0x2032] = "\\ensuremath{\\prime}",
  [0x2082] = "\\textsubscript{2}",
  [0x2084] = "\\textsubscript{4}",
  [0x2086] = "\\textsubscript{6}",
  [0x20AC] = "\\euro{}",
  [0x2192] = "\\ensuremath{\\rightarrow}",
  [0x25D0] = "\\halfcircle",
  [0x2713] = "\\ensuremath{\\checkmark}",
  [0x2717] = "\\xmark",
}

local code_replacements = {
  ["\u{2082}"] = "2",
  ["\u{2084}"] = "4",
  ["\u{2086}"] = "6",
  ["\u{2192}"] = "->",
}

local function needs_symbol_command(text)
  for _, code in utf8.codes(text) do
    if symbol_commands[code] then
      return true
    end
  end
  return false
end

function Str(el)
  if not needs_symbol_command(el.text) then
    return nil
  end
  local pieces = pandoc.Inlines({})
  for _, code in utf8.codes(el.text) do
    local command = symbol_commands[code]
    if command then
      pieces:insert(pandoc.RawInline("latex", command))
    else
      pieces:insert(pandoc.Str(utf8.char(code)))
    end
  end
  return pieces
end

function CodeBlock(el)
  local text = el.text
  for char, replacement in pairs(code_replacements) do
    text = text:gsub(char, replacement)
  end
  if text == el.text then
    return nil
  end
  el.text = text
  return el
end

function Pandoc(doc)
  local body = pandoc.Blocks({})
  local past_title_block = false
  for _, block in ipairs(doc.blocks) do
    if past_title_block then
      if block.tag == "Header" and block.level > 1 then
        block.level = block.level - 1
      end
      body:insert(block)
    elseif block.tag == "HorizontalRule" then
      past_title_block = true
    end
  end
  if not past_title_block then
    return nil
  end
  return pandoc.Pandoc(body, doc.meta)
end
