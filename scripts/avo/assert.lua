local script_dir = (arg[0] or ""):match("^(.*)/[^/]+$") or "."
local json = dofile(script_dir .. "/vendor/json.lua")
local mode = arg[1]

local function read_rows(path)
  local file, err = io.open(path, "r")
  if not file then error(err) end
  local rows = setmetatable({}, json.array_mt)
  for line in file:lines() do
    if line:match("%S") then rows[#rows + 1] = json.decode(line) end
  end
  file:close()
  return rows
end

local function matches(row, first_filter)
  for i = first_filter, #arg do
    local key, raw = arg[i]:match("^([^=]+)=(.*)$")
    assert(key, "expected key=value filter: " .. arg[i])
    local value = row[key]
    if raw == "present" then
      if value == nil or value == json.null or value == false or value == "" then return false end
    elseif key == "kind" and raw:find("|", 1, true) then
      local found = false
      for kind in raw:gmatch("[^|]+") do
        if value == kind then found = true end
      end
      if not found then return false end
    else
      local expected
      if raw == "true" then
        expected = true
      elseif raw == "false" then
        expected = false
      elseif raw == "null" then
        expected = json.null
      else
        expected = tonumber(raw) or raw
      end
      if value ~= expected then return false end
    end
  end
  return true
end

if mode == "score" then
  local expected = arg[2]
  local score = json.decode(io.read("*a"))
  if expected == "valid" then
    assert(score.correct == true)
    assert(type(score.objective) == "number")
    assert(type(score.metrics) == "table" and getmetatable(score.metrics) == json.object_mt)
    assert(type(score.note) == "string" and score.note ~= "")
    assert(type(score.artifacts) == "table" and getmetatable(score.artifacts) == json.array_mt)
  elseif expected == "incorrect" then
    assert(score.correct == false and score.objective == 0)
    assert(score.note == "correctness command failed" and #score.artifacts == 0)
  elseif expected == "timed-valid" then
    assert(score.correct == true and score.objective > 0)
    assert(type(score.metrics.elapsed_s) == "number")
  elseif expected == "timed-incorrect" then
    assert(score.correct == false and score.objective == 0)
    assert(score.note == "tests failed")
  else
    error("unknown score assertion: " .. tostring(expected))
  end
elseif mode == "ledger" then
  local action, path = arg[2], arg[3]
  local rows = read_rows(path)
  local first_filter = 4
  local expected_count
  if action == "count" then
    expected_count = tonumber(arg[4])
    assert(expected_count, "count assertion requires an integer")
    first_filter = 5
  end
  local count, found = 0, false
  for _, row in ipairs(rows) do
    if matches(row, first_filter) then
      count = count + 1
      found = true
    end
  end
  if action == "has" then
    assert(found, "expected a matching ledger row")
  elseif action == "none" then
    assert(not found, "unexpected matching ledger row")
  elseif action == "count" then
    assert(count == expected_count, "unexpected matching ledger row count: " .. count)
  elseif action == "last" then
    assert(#rows > 0 and matches(rows[#rows], first_filter), "unexpected final ledger row")
  else
    error("unknown ledger assertion: " .. tostring(action))
  end
else
  error("unknown assertion mode: " .. tostring(mode))
end
