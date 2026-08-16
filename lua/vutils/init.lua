local M = {}

local defaults = {
  command = nil,
  args = { "tui" },
  keymap = "<leader>uv",
  width = 0.9,
  height = 0.85,
  border = "rounded",
  winblend = 0,
}

local config = vim.deepcopy(defaults)
local state = { buffer = nil, window = nil, job = nil }

local function notify(message, level)
  vim.notify(message, level or vim.log.levels.INFO, { title = "vutils" })
end

local function plugin_binary()
  local source = debug.getinfo(1, "S").source
  if source:sub(1, 1) ~= "@" then
    return nil
  end

  local module_file = source:sub(2)
  local root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(module_file)))
  local executable = root .. "/target/release/vutils"
  if vim.fn.has("win32") == 1 then
    executable = executable .. ".exe"
  end
  return vim.fn.executable(executable) == 1 and executable or nil
end

local function resolve_command()
  if config.command then
    return vim.fn.executable(config.command) == 1 and config.command or nil
  end
  local bundled = plugin_binary()
  if bundled then
    return bundled
  end
  if vim.fn.executable("vutils") == 1 then
    return "vutils"
  end
  return nil
end

local function close(stop_job)
  local job = state.job
  local window = state.window
  local buffer = state.buffer
  state.job, state.window, state.buffer = nil, nil, nil

  if stop_job and job and job > 0 then
    pcall(vim.fn.jobstop, job)
  end
  if window and vim.api.nvim_win_is_valid(window) then
    pcall(vim.api.nvim_win_close, window, true)
  end
  if buffer and vim.api.nvim_buf_is_valid(buffer) then
    pcall(vim.api.nvim_buf_delete, buffer, { force = true })
  end
end

local function dimension(value, available, minimum)
  local calculated = value <= 1 and math.floor(available * value) or math.floor(value)
  local maximum = math.max(1, available - 2)
  return math.min(math.max(minimum, calculated), maximum)
end

function M.open()
  if state.window and vim.api.nvim_win_is_valid(state.window) then
    vim.api.nvim_set_current_win(state.window)
    vim.cmd("startinsert")
    return
  end

  local executable = resolve_command()
  if not executable then
    notify(
      "vutils was not found. Install it on PATH or enable `build = \"cargo build --release --locked\"` in the Lazy spec.",
      vim.log.levels.ERROR
    )
    return
  end

  local width = dimension(config.width, vim.o.columns, 72)
  local height = dimension(config.height, vim.o.lines - vim.o.cmdheight, 20)
  local row = math.max(0, math.floor((vim.o.lines - height) / 2) - 1)
  local column = math.max(0, math.floor((vim.o.columns - width) / 2))
  local buffer = vim.api.nvim_create_buf(false, true)
  vim.bo[buffer].bufhidden = "wipe"

  local window = vim.api.nvim_open_win(buffer, true, {
    relative = "editor",
    row = row,
    col = column,
    width = width,
    height = height,
    style = "minimal",
    border = config.border,
    title = " vutils ",
    title_pos = "center",
    zindex = 50,
  })
  vim.wo[window].winblend = config.winblend
  state.buffer, state.window = buffer, window

  local command = { executable }
  vim.list_extend(command, vim.deepcopy(config.args))
  local job = vim.fn.jobstart(command, {
    term = true,
    cwd = vim.fn.getcwd(),
    on_exit = function(job_id)
      vim.schedule(function()
        if state.job == job_id then
          close(false)
        end
      end)
    end,
  })
  if job <= 0 then
    close(false)
    notify("Failed to start `" .. executable .. " tui`.", vim.log.levels.ERROR)
    return
  end
  state.job = job

  vim.keymap.set("t", "<Esc><Esc>", function()
    close(true)
  end, { buffer = buffer, desc = "Close vutils TUI" })
  vim.keymap.set("n", "q", function()
    close(true)
  end, { buffer = buffer, desc = "Close vutils TUI" })
  vim.api.nvim_create_autocmd("BufWipeout", {
    buffer = buffer,
    once = true,
    callback = function()
      if state.buffer == buffer then
        local active_job = state.job
        state.job, state.window, state.buffer = nil, nil, nil
        if active_job and active_job > 0 then
          pcall(vim.fn.jobstop, active_job)
        end
      end
    end,
  })
  vim.cmd("startinsert")
end

function M.close()
  close(true)
end

function M.setup(opts)
  config = vim.tbl_deep_extend("force", vim.deepcopy(defaults), opts or {})
  if type(config.width) ~= "number" or config.width <= 0 then
    error("vutils: `width` must be a positive number")
  end
  if type(config.height) ~= "number" or config.height <= 0 then
    error("vutils: `height` must be a positive number")
  end
  if config.command ~= nil and type(config.command) ~= "string" then
    error("vutils: `command` must be a string or nil")
  end
  if type(config.args) ~= "table" then
    error("vutils: `args` must be a list")
  end
  for index, argument in ipairs(config.args) do
    if type(argument) ~= "string" then
      error("vutils: `args[" .. index .. "]` must be a string")
    end
  end
  if type(config.winblend) ~= "number" or config.winblend < 0 or config.winblend > 100 then
    error("vutils: `winblend` must be between 0 and 100")
  end
  vim.api.nvim_create_user_command("Vutils", M.open, {
    desc = "Open the vutils terminal interface",
    force = true,
  })
  if type(config.keymap) == "string" and config.keymap ~= "" then
    vim.keymap.set("n", config.keymap, M.open, { desc = "Open vutils TUI" })
  end
end

return M
