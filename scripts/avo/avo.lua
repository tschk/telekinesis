local script_dir = (arg[0] or ""):match("^(.*)/[^/]+$") or "."
package.path = script_dir .. "/vendor/?.lua;" .. package.path

local json = require("json")
local command = arg[1]

local function read_file(path)
  local file, err = io.open(path, "r")
  if not file then
    if err and err:match("No such file") then return nil end
    error(err)
  end
  local value = file:read("*a")
  file:close()
  return value
end

local function read_rows(path)
  local contents = read_file(path)
  local rows = setmetatable({}, json.array_mt)
  if not contents then return rows end
  for line in contents:gmatch("[^\r\n]+") do
    if line:match("%S") then rows[#rows + 1] = json.decode(line) end
  end
  return rows
end

local function number(value, label)
  local result = tonumber(value)
  if not result or result ~= result or result == math.huge or result == -math.huge then
    error("invalid number for " .. label .. ": " .. tostring(value))
  end
  return result
end

local function number_text(value, force_float)
  local result = json.encode(value)
  if force_float and result:find("^[+-]?%d+$") then result = result .. ".0" end
  return result
end

local function python_string(value)
  if value == nil or value == json.null then return "None" end
  if type(value) == "boolean" then return value and "True" or "False" end
  if type(value) == "number" then return number_text(value, false) end
  if type(value) == "string" then return value end
  return json.encode(value)
end

local function python_truthy(value)
  if value == nil or value == json.null or value == false then return false end
  if type(value) == "number" then return value ~= 0 end
  if type(value) == "string" then return value ~= "" end
  if type(value) == "table" then return next(value) ~= nil end
  return true
end

local function number_from_row(row)
  return number(row.objective == nil and 0 or row.objective, "objective")
end

local function output(value)
  io.write(tostring(value), "\n")
end

local function config_write()
  local keys = { "task", "goal", "score", "agent", "branch" }
  io.write("{\n")
  for i, key in ipairs(keys) do
    io.write("  ", json.encode(key), ": ", json.encode(arg[i + 1]))
    io.write(i == #keys and "\n" or ",\n")
  end
  io.write("}\n")
end

local function json_object()
  local fields = {}
  for i = 2, #arg do
    local kind, key, raw = arg[i]:match("^([sbnf]):([^=]+)=(.*)$")
    if not kind then error("expected typed key=value argument: " .. arg[i]) end
    local encoded
    if kind == "s" then
      encoded = json.encode(raw)
    elseif kind == "b" then
      if raw ~= "true" and raw ~= "false" then error("invalid boolean: " .. raw) end
      encoded = raw
    elseif kind == "f" then
      encoded = number_text(number(raw, key), true)
    else
      encoded = json.encode(number(raw, key))
    end
    fields[#fields + 1] = json.encode(key) .. ":" .. encoded
  end
  output("{" .. table.concat(fields, ",") .. "}")
end

local function ledger_append()
  local path, kind, payload = arg[2], arg[3], arg[4]
  local row = json.decode(payload)
  if type(row) ~= "table" or getmetatable(row) ~= json.object_mt then
    error("ledger payload must be a JSON object")
  end
  local preserve_payload = not payload:match('"kind"%s*:') and not payload:match('"ts"%s*:')
  row.kind = kind
  if row.ts == nil then row.ts = os.date("!%Y-%m-%dT%H:%M:%SZ") end
  local file, err = io.open(path, "a")
  if not file then error(err) end
  if preserve_payload then
    file:write(payload:sub(1, -2), payload == "{}" and "" or ",",
      '"kind":', json.encode(kind), ',"ts":', json.encode(row.ts), "}\n")
  else
    file:write(json.encode(row), "\n")
  end
  file:close()
end

local function best_objective(path)
  local best
  for _, row in ipairs(read_rows(path)) do
    if row.kind == "accept" or row.kind == "baseline" then
      local objective = number_from_row(row)
      if best == nil or objective > best then best = objective end
    end
  end
  output(best == nil and "" or number_text(best, true))
end

local function should_supervise(path, stall_after)
  stall_after = number(stall_after, "stall threshold")
  if stall_after % 1 ~= 0 then error("stall threshold must be an integer") end
  local best, since_best, since_supervisor = nil, 0, 0
  for _, row in ipairs(read_rows(path)) do
    local kind = row.kind
    if kind == "supervisor" then
      since_supervisor = 0
    elseif kind == "accept" or kind == "reject" or kind == "error" then
      since_supervisor = since_supervisor + 1
      local objective = number_from_row(row)
      local improved = kind == "accept" and (best == nil or objective > best)
      if improved then
        best = objective
        since_best = 0
      else
        since_best = since_best + 1
      end
    end
  end
  output(since_best >= stall_after and since_supervisor >= stall_after and "yes" or "no")
end

local function parse_score(raw)
  local score = json.decode(raw)
  if type(score) ~= "table" or getmetatable(score) ~= json.object_mt then
    error("score JSON must be an object")
  end
  if score.correct == nil or score.objective == nil or score.note == nil then
    error("score JSON must include correct, objective, note")
  end
  if type(score.correct) ~= "boolean" then
    error('score JSON "correct" must be a JSON boolean')
  end
  local objective = score.correct and number(score.objective, "objective") or 0
  local metrics = score.metrics
  if not python_truthy(metrics) then metrics = setmetatable({}, json.object_mt) end
  if type(metrics) ~= "table" or getmetatable(metrics) == json.array_mt then
    error("score JSON metrics must be an object")
  end
  local stddev = metrics.stddev
  if stddev == nil then stddev = metrics.sigma end
  if not python_truthy(stddev) then stddev = 0 end
  local parsed = setmetatable({
    correct = score.correct,
    objective = objective,
    note = python_string(score.note),
    stddev = number(stddev, "stddev"),
    raw = score,
  }, json.object_mt)
  output(json.encode(parsed))
end

local function should_commit(correct, objective, stddev, best, minimum, noise)
  if correct ~= "true" or best == "" then
    output("no")
    return
  end
  objective = number(objective, "objective")
  stddev = number(stddev == "" and "0" or stddev, "stddev")
  minimum = number(minimum == "" and "0" or minimum, "minimum improvement")
  noise = number(noise == "" and "1" or noise, "noise multiplier")
  output(objective + noise * stddev >= number(best, "best objective") + minimum and "yes" or "no")
end

local function lineage(path, count)
  count = number(count, "lineage length")
  if count % 1 ~= 0 then error("lineage length must be an integer") end
  local rows = read_rows(path)
  local scored = setmetatable({}, json.array_mt)
  for _, row in ipairs(rows) do
    if row.kind == "accept" or row.kind == "reject" or row.kind == "baseline" then
      scored[#scored + 1] = row
    end
  end
  local first
  if count == 0 then
    first = 1
  elseif count > 0 then
    first = math.max(1, #scored - count + 1)
  else
    first = math.max(1, -count + 1)
  end
  if first > #scored then
    output("(empty — this is the first variation)")
    return
  end
  for i = first, #scored do
    local row = scored[i]
    local candidate = python_truthy(row.candidate) and row.candidate
      or (python_truthy(row.commit) and row.commit or "-")
    local patch = python_truthy(row.patch) and row.patch or ""
    local extra = " candidate=" .. python_string(candidate)
    if patch ~= "" then extra = extra .. " patch=" .. python_string(patch) end
    io.write("tick ", python_string(row.tick), " kind=", python_string(row.kind),
      " correct=", python_string(row.correct), " objective=", number_text(number_from_row(row), true),
      " note=", python_string(row.note), extra, "\n")
  end
end

local function status(task, branch, path, stall_after)
  stall_after = number(stall_after, "stall threshold")
  if stall_after % 1 ~= 0 then error("stall threshold must be an integer") end
  local rows = read_rows(path)
  local scored = setmetatable({}, json.array_mt)
  for _, row in ipairs(rows) do
    if row.kind == "accept" or row.kind == "reject" or row.kind == "error" then
      scored[#scored + 1] = row
    end
  end
  local best, best_tick, since_best = nil, nil, 0
  for _, row in ipairs(rows) do
    if row.kind == "baseline" then
      local objective = number_from_row(row)
      if best == nil or objective > best then
        best = objective
        best_tick = row.tick == nil and 0 or row.tick
      end
    end
  end
  for _, row in ipairs(scored) do
    local objective = number_from_row(row)
    if row.kind == "accept" and (best == nil or objective > best) then
      best = objective
      best_tick = row.tick
      since_best = 0
    else
      since_best = since_best + 1
    end
  end
  io.write("task: ", task, "\nbranch: ", branch, "\nticks: ", #scored, "\n")
  if best == nil then
    io.write("best: none\n")
  else
    io.write("best: ", number_text(best, true), " (tick ", python_string(best_tick), ")\n")
  end
  io.write("stall: ", since_best, "/", stall_after, "\nlineage:\n")
  local first = math.max(1, #scored - 7)
  for i = first, #scored do
    local row = scored[i]
    local objective = number_from_row(row)
    local objective_text = row.kind == "error" and python_string(objective) or number_text(objective, true)
    io.write("  ", python_string(row.tick), " ", python_string(row.kind),
      " objective=", objective_text, " correct=", python_string(row.correct),
      " note=", python_string(row.note), "\n")
  end
end

local function test_score(correct, objective, note, stddev)
  io.write('{"correct": ', tostring(correct == "true"), ', "objective": ',
    number_text(number(objective, "objective"), true), ', "metrics": {"stddev": ',
    number_text(number(stddev or "0", "stddev"), true), '}, "note": ', json.encode(note),
    ', "artifacts": []}\n')
end

local commands = {
  ["config-get"] = function()
    local value = json.decode(read_file(arg[2]))
    local key = arg[3]
    if key == "*" then output(json.encode(value)) else output(python_string(value[key] == nil and "" or value[key])) end
  end,
  ["config-write"] = config_write,
  ["json-object"] = json_object,
  ["ledger-append"] = ledger_append,
  ["ledger-rows"] = function() output(json.encode(read_rows(arg[2]))) end,
  ["best-objective"] = function() best_objective(arg[2]) end,
  ["has-baseline"] = function()
    for _, row in ipairs(read_rows(arg[2])) do
      if row.kind == "baseline" or row.kind == "accept" then output("yes"); return end
    end
    output("no")
  end,
  ["tick-number"] = function()
    local count = 0
    for _, row in ipairs(read_rows(arg[2])) do
      if row.kind == "accept" or row.kind == "reject" or row.kind == "error" then count = count + 1 end
    end
    output(count + 1)
  end,
  ["should-supervise"] = function() should_supervise(arg[2], arg[3]) end,
  ["parse-score"] = function() parse_score(arg[2]) end,
  ["should-commit"] = function() should_commit(arg[2], arg[3], arg[4], arg[5], arg[6], arg[7]) end,
  ["lineage"] = function() lineage(arg[2], arg[3]) end,
  ["status"] = function() status(arg[2], arg[3], arg[4], arg[5]) end,
  ["is-number"] = function()
    local value = tonumber(arg[2])
    if not value or value ~= value or value == math.huge or value == -math.huge then os.exit(1) end
  end,
  ["elapsed"] = function()
    local elapsed = number(arg[3], "end time") - number(arg[2], "start time")
    output(json.encode(math.max(elapsed, 0.000001)))
  end,
  ["reciprocal"] = function() output(json.encode(1 / number(arg[2], "elapsed time"))) end,
  ["monotonic-time"] = function()
    local ffi = require("ffi")
    ffi.cdef[[
      typedef struct { long tv_sec; long tv_nsec; } avo_timespec;
      int clock_gettime(int clock_id, avo_timespec *time);
    ]]
    local clock_id = jit.os == "OSX" and 6 or 1
    local time = ffi.new("avo_timespec[1]")
    if ffi.C.clock_gettime(clock_id, time) ~= 0 then error("clock_gettime failed") end
    output(string.format("%.9f", tonumber(time[0].tv_sec) + tonumber(time[0].tv_nsec) / 1000000000))
  end,
  ["score-output"] = function()
    local correct = arg[2] == "true"
    local objective = number(arg[3], "objective")
    local note = arg[4]
    local elapsed = number(arg[5], "elapsed")
    local ran_test = arg[6] == "true"
    local metrics = ran_test and '{"elapsed_s": ' .. number_text(elapsed, true) .. "}" or "{}"
    io.write('{"correct": ', tostring(correct), ', "objective": ', number_text(objective, true),
      ', "metrics": ', metrics, ', "note": ', json.encode(note), ', "artifacts": []}\n')
  end,
  ["test-score"] = function() test_score(arg[2], arg[3], arg[4], arg[5]) end,
  ["json-get"] = function()
    local value = json.decode(arg[2])
    local selected = value[arg[3]]
    if type(selected) == "boolean" then
      output(tostring(selected))
    elseif arg[4] == "float" and type(selected) == "number" then
      output(number_text(selected, true))
    else
      output(python_string(selected == nil and "" or selected))
    end
  end,
}

local selected = commands[command]
if not selected then error("unknown AVO command: " .. tostring(command)) end
selected()
