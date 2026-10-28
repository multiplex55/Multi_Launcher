local M = {}

local PROFILE_NAMES = { light = true, full = true }
local MODULE_PATH = debug.getinfo(1, 'S').source:sub(2)
local REPOSITORY_ROOT = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(MODULE_PATH)))
local state

local function is_windows()
  return vim.uv.os_uname().sysname:lower():find('windows', 1, true) ~= nil
end

local function canonical_path(path)
  if type(path) ~= 'string' or path == '' then
    return nil
  end
  local resolved = vim.uv.fs_realpath(path) or vim.fs.normalize(path)
  resolved = resolved:gsub('\\', '/'):gsub('/+', '/')
  if #resolved > 3 then
    resolved = resolved:gsub('/+$', '')
  end
  return is_windows() and resolved:lower() or resolved
end

local function same_root(left, right)
  local left_path = canonical_path(left)
  local right_path = canonical_path(right)
  return left_path ~= nil and left_path == right_path
end

local function path_is_within(path, root)
  local normalized_path = canonical_path(path)
  local normalized_root = canonical_path(root)
  if not normalized_path or not normalized_root then
    return false
  end
  if normalized_path == normalized_root then
    return true
  end
  local prefix = normalized_root:sub(-1) == '/' and normalized_root or (normalized_root .. '/')
  return normalized_path:sub(1, #prefix) == prefix
end

local function copy(value)
  return type(value) == 'table' and vim.deepcopy(value) or value
end

local function command_args(root, cargo, profile)
  assert(type(root) == 'string' and root ~= '', 'root must be a non-empty path')
  assert(type(cargo) == 'string' and cargo ~= '', 'cargo must be a resolved executable')
  assert(PROFILE_NAMES[profile], 'profile must be light or full')
  local manifest = vim.fs.joinpath(root, 'Cargo.toml')
  local args = { 'check', '--manifest-path', manifest }
  if profile == 'light' then
    vim.list_extend(args, { '-p', 'multi_launcher', '--lib', '--bin', 'multi_launcher' })
  else
    vim.list_extend(args, { '--workspace', '--all-targets' })
  end
  vim.list_extend(args, { '--message-format=json' })
  return { cargo, unpack(args) }
end

local function rust_analyzer_settings(command)
  return {
    ['rust-analyzer'] = {
      cargo = {
        allFeatures = false,
        buildScripts = { enable = true },
      },
      procMacro = { enable = true },
      diagnostics = { enable = true },
      checkOnSave = true,
      check = {
        command = 'check',
        overrideCommand = command,
      },
      cfg = { setTest = true },
    },
  }
end

local function profile_settings(root, cargo, profile)
  return rust_analyzer_settings(command_args(root, cargo, profile))
end

local function remove_inherited_conflicts(settings)
  local ra = settings['rust-analyzer']
  if type(ra) ~= 'table' then
    return settings
  end
  if type(ra.check) == 'table' then
    ra.check.enable = nil
  end
  if type(ra.cargo) == 'table' then
    ra.cargo.loadOutDirsFromCheck = nil
  end
  return settings
end

local function active_clients()
  if type(vim.lsp.get_clients) ~= 'function' then
    return {}
  end
  return vim.lsp.get_clients({ name = 'rust-analyzer' })
end

local function effective_server_settings(root, default_settings)
  local current = vim.g.rustaceanvim
  local server = type(current) == 'table' and current.server or nil
  local settings = type(server) == 'table' and server.settings or nil
  if type(settings) == 'function' then
    settings = settings(root, default_settings)
  end
  if type(settings) ~= 'table' then
    settings = default_settings
  end
  return type(settings) == 'table' and copy(settings) or {}
end

local function compose_original_settings(current_state, root, default_settings)
  local base = effective_server_settings(root, default_settings)
  if not current_state.original_lsp_settings_present then
    return base
  end

  local previous = current_state.original_lsp_settings
  if type(previous) == 'function' then
    local result = previous(root, default_settings)
    return type(result) == 'table' and copy(result) or {}
  end
  if type(previous) == 'table' then
    return vim.tbl_deep_extend('force', base, copy(previous))
  end
  return base
end

local function default_auto_attach(bufnr)
  if not vim.api.nvim_buf_is_valid(bufnr) or #vim.bo[bufnr].buftype > 0 then
    return false
  end
  local filename = vim.api.nvim_buf_get_name(bufnr)
  if filename == '' or filename:match('^%a[%w+.-]*://') then
    return false
  end
  local current = vim.g.rustaceanvim
  local server = type(current) == 'table' and current.server or nil
  local configured_cmd = type(server) == 'table' and server.cmd or nil
  if type(configured_cmd) == 'function' then
    -- Match Rustaceanvim's default: connection callbacks are not evaluated for
    -- the auto-attach executable check.
    return true
  end
  local executable = type(configured_cmd) == 'table' and configured_cmd[1] or 'rust-analyzer'
  return type(executable) == 'string' and vim.fn.executable(executable) == 1
end

local function delegate_auto_attach(current_state, bufnr)
  if current_state.original_auto_attach_present then
    if type(current_state.original_auto_attach) == 'function' then
      return current_state.original_auto_attach(bufnr)
    end
    return current_state.original_auto_attach == true
  end
  return default_auto_attach(bufnr)
end

local function buffer_cargo_root(bufnr)
  if not vim.api.nvim_buf_is_valid(bufnr) then
    return nil
  end
  local filename = vim.api.nvim_buf_get_name(bufnr)
  if filename == '' or filename:match('^%a[%w+.-]*://') then
    return nil
  end
  local root = vim.fs.root(filename, 'Cargo.toml')
  return root and canonical_path(root) or nil
end

local function workspace_package_roots(root)
  local manifest_path = vim.fs.joinpath(root, 'Cargo.toml')
  local readable, lines = pcall(vim.fn.readfile, manifest_path)
  if not readable or #lines == 0 then
    return nil, 'repository Cargo.toml could not be read'
  end

  -- The opt-in adapter accepts only explicit, literal workspace members from
  -- this repository manifest. It intentionally does not expand globs or parse
  -- general TOML, so an unrelated nested Cargo project is never admitted by
  -- virtue of being somewhere below the repository root.
  local in_workspace = false
  local members_value
  for _, line in ipairs(lines) do
    if line:match('^%s*%[workspace%]%s*$') then
      in_workspace = true
    elseif in_workspace and line:match('^%s*%[') then
      break
    elseif in_workspace then
      local excluded = line:match('^%s*exclude%s*=')
      if excluded then
        return nil, 'workspace exclusions are unsupported by the adapter allowlist'
      end
      local value = line:match('^%s*members%s*=%s*(.-)%s*$')
      if value then
        if members_value then
          return nil, 'workspace declares members more than once'
        end
        members_value = value
      end
    end
  end

  local members_body = members_value and members_value:match('^%[(.*)%]$')
  if not members_body then
    return nil, 'workspace members must be a simple inline list of literal paths'
  end
  local member_paths = {}
  for member in members_body:gmatch('"([^"]*)"') do
    if member == '' or member:find('\\', 1, true) or member:find('*', 1, true)
      or member:find('?', 1, true) or member:find('{', 1, true) or member:find('}', 1, true)
      or member:sub(1, 1) == '/' or member:match('^%a:')
      or member == '..' or member:match('^%.%./') or member:find('/../', 1, true)
      or member:match('/%.%.$')
    then
      return nil, 'workspace member path is not a safe literal relative path'
    end
    member_paths[#member_paths + 1] = member
  end
  local normalized_members = members_body:gsub('%s', '')
  local expected_members = {}
  for _, member in ipairs(member_paths) do
    expected_members[#expected_members + 1] = '"' .. member .. '"'
  end
  if #member_paths == 0 or normalized_members ~= table.concat(expected_members, ',') then
    return nil, 'workspace members use an unsupported TOML shape'
  end

  local allowed_roots = {}
  for _, member in ipairs(member_paths) do
    local package_root = canonical_path(vim.fs.joinpath(root, member))
    if not package_root or not path_is_within(package_root, root) then
      return nil, 'workspace member resolved outside the repository root'
    end
    local package_manifest = vim.fs.joinpath(package_root, 'Cargo.toml')
    if vim.fn.filereadable(package_manifest) ~= 1 then
      return nil, 'workspace member Cargo.toml is unavailable: ' .. member
    end
    local package_readable, package_lines = pcall(vim.fn.readfile, package_manifest)
    local declares_package = false
    if package_readable then
      for _, line in ipairs(package_lines) do
        if line:match('^%s*%[package%]%s*$') then
          declares_package = true
          break
        end
      end
    end
    if not declares_package then
      return nil, 'workspace member is not a readable Cargo package: ' .. member
    end
    allowed_roots[package_root] = true
  end
  if not allowed_roots[canonical_path(root)] then
    return nil, 'workspace members do not explicitly include the repository package'
  end
  return allowed_roots
end

local function install_commands(current_state)
  vim.api.nvim_create_user_command('MultiLauncherRustAnalyzerProfile', function(opts)
    local ok, err = M.set_profile(opts.args)
    if not ok then
      vim.notify(err, vim.log.levels.ERROR)
      return
    end
    vim.notify('Multi Launcher rust-analyzer profile: ' .. opts.args)
  end, { nargs = 1, complete = function() return { 'light', 'full' } end })

  vim.api.nvim_create_user_command('MultiLauncherRustAnalyzerRevert', function()
    local ok, err = M.revert()
    if not ok then
      vim.notify(err, vim.log.levels.ERROR)
      return
    end
    vim.notify('Multi Launcher rust-analyzer adapter reverted')
  end, {})

  vim.api.nvim_create_user_command('MultiLauncherRustAnalyzerStatus', function()
    vim.notify(vim.inspect(M.status()))
  end, {})
  current_state.commands_installed = true
end

local function update_global_auto_attach(new_value)
  local current = vim.g.rustaceanvim
  if type(current) == 'function' then
    return false, 'vim.g.rustaceanvim became a function; refusing to replace it'
  end
  current = type(current) == 'table' and copy(current) or {}
  local server = type(current.server) == 'table' and copy(current.server) or {}
  server.auto_attach = new_value
  current.server = server
  vim.g.rustaceanvim = current
  return true
end

function M.normalize_path(path)
  return canonical_path(path)
end

function M.same_root(left, right)
  return same_root(left, right)
end

function M.path_is_within(path, root)
  return path_is_within(path, root)
end

function M.command_args(root, cargo, profile)
  return command_args(root, cargo, profile)
end

function M.merge_profile(base, root, cargo, profile)
  local merged = vim.tbl_deep_extend('force', copy(base or {}), profile_settings(root, cargo, profile))
  return remove_inherited_conflicts(merged)
end

function M.apply(root)
  if state and state.active then
    return false, 'adapter is already active in this Neovim session'
  end
  if type(vim.lsp.config) ~= 'table' then
    return false, 'Neovim public vim.lsp.config is unavailable'
  end
  if package.loaded['rustaceanvim.config.internal'] or vim.g.loaded_rustaceanvim then
    return false, 'Rustaceanvim configuration was already cached; apply before the first Rust buffer'
  end
  if #active_clients() > 0 then
    return false, 'a rust-analyzer client is already active; refusing late configuration'
  end

  local expected_root = canonical_path(REPOSITORY_ROOT)
  local requested_root = canonical_path(root)
  local current_dir = canonical_path(vim.uv.cwd())
  if not expected_root or requested_root ~= expected_root then
    return false, 'adapter root must be the repository containing this file'
  end
  if current_dir ~= requested_root then
    return false, 'start Neovim with the repository root as its working directory'
  end
  if vim.fn.filereadable(vim.fs.joinpath(root, 'Cargo.toml')) ~= 1 then
    return false, 'repository Cargo.toml is unavailable'
  end
  local allowed_package_roots, workspace_error = workspace_package_roots(requested_root)
  if not allowed_package_roots then
    return false, workspace_error
  end
  local cargo = vim.fn.exepath('cargo')
  if cargo == '' then
    return false, 'cargo is not available on PATH'
  end
  if vim.api.nvim_get_commands({})['MultiLauncherRustAnalyzerProfile']
    or vim.api.nvim_get_commands({})['MultiLauncherRustAnalyzerRevert']
    or vim.api.nvim_get_commands({})['MultiLauncherRustAnalyzerStatus']
  then
    return false, 'a Multi Launcher rust-analyzer command already exists'
  end

  local gopts = vim.g.rustaceanvim
  if type(gopts) == 'function' then
    return false, 'function-valued vim.g.rustaceanvim is not supported by this adapter'
  end
  local server = type(gopts) == 'table' and gopts.server or nil
  if server ~= nil and type(server) ~= 'table' then
    return false, 'vim.g.rustaceanvim.server must be a table'
  end

  local public_config = vim.lsp.config['rust-analyzer']
  if public_config ~= nil and type(public_config) ~= 'table' then
    return false, 'vim.lsp.config["rust-analyzer"] must be a table'
  end
  local current_state = {
    active = true,
    root = requested_root,
    cargo = cargo,
    profile = 'light',
    allowed_package_roots = allowed_package_roots,
    original_auto_attach_present = type(server) == 'table' and rawget(server, 'auto_attach') ~= nil,
    original_auto_attach = nil,
    original_lsp_settings_present = type(public_config) == 'table' and rawget(public_config, 'settings') ~= nil,
    original_lsp_settings = type(public_config) == 'table' and copy(rawget(public_config, 'settings')) or nil,
    last_refusal = nil,
  }
  if type(server) == 'table' then
    current_state.original_auto_attach = copy(rawget(server, 'auto_attach'))
  end

  current_state.auto_attach = function(bufnr)
    if not current_state.active then
      return delegate_auto_attach(current_state, bufnr)
    end
    local buffer_root = buffer_cargo_root(bufnr)
    if not buffer_root or not current_state.allowed_package_roots[buffer_root] then
      current_state.last_refusal = {
        bufnr = bufnr,
        filename = vim.api.nvim_buf_get_name(bufnr),
        buffer_root = buffer_root,
        expected_root = current_state.root,
        reason = not buffer_root and 'no-cargo-root'
          or path_is_within(buffer_root, current_state.root) and 'unlisted-nested-cargo-root'
          or 'different-cargo-root',
      }
      return false
    end
    current_state.last_refusal = nil
    return delegate_auto_attach(current_state, bufnr)
  end

  current_state.settings = function(project_root, default_settings)
    if not current_state.active or not same_root(project_root, current_state.root) then
      return compose_original_settings(current_state, project_root, default_settings)
    end
    local base = compose_original_settings(current_state, project_root, default_settings)
    local overlay = profile_settings(current_state.root, current_state.cargo, current_state.profile)
    return remove_inherited_conflicts(vim.tbl_deep_extend('force', base, overlay))
  end

  local ok, err = update_global_auto_attach(current_state.auto_attach)
  if not ok then
    return false, err
  end
  local current_entry = type(public_config) == 'table' and copy(public_config) or {}
  current_entry.settings = current_state.settings
  vim.lsp.config['rust-analyzer'] = current_entry
  state = current_state
  package.loaded['multi_launcher.rust_analyzer_profile'] = M
  install_commands(current_state)
  return true, 'applied light profile for ' .. requested_root
end

function M.set_profile(profile)
  if not state or not state.active then
    return false, 'adapter is not active'
  end
  if not PROFILE_NAMES[profile] then
    return false, 'profile must be light or full'
  end
  if not same_root(vim.uv.cwd(), state.root) then
    return false, 'profile changes are restricted to the adapter root session'
  end
  local previous = state.profile
  state.profile = profile
  if #active_clients() > 0 then
    local ok, lsp = pcall(require, 'rustaceanvim.lsp')
    if not ok or type(lsp.restart) ~= 'function' then
      state.profile = previous
      return false, 'Rustaceanvim restart API is unavailable; profile was not changed'
    end
    local restart_ok, restart_error = pcall(lsp.restart)
    if not restart_ok then
      state.profile = previous
      return false, 'Rustaceanvim restart failed: ' .. tostring(restart_error)
    end
  end
  return true
end

function M.revert()
  if not state or not state.active then
    return false, 'adapter is not active'
  end
  if #active_clients() > 0 then
    return false, 'stop rust-analyzer clients before reverting adapter settings'
  end
  if not same_root(vim.uv.cwd(), state.root) then
    return false, 'reversion is restricted to the adapter root session'
  end

  local current_global = vim.g.rustaceanvim
  local current_server = type(current_global) == 'table' and current_global.server or nil
  if type(current_server) ~= 'table' or rawget(current_server, 'auto_attach') ~= state.auto_attach then
    return false, 'vim.g.rustaceanvim.server.auto_attach changed after adapter activation'
  end
  local current_entry = vim.lsp.config['rust-analyzer']
  if type(current_entry) ~= 'table' or rawget(current_entry, 'settings') ~= state.settings then
    return false, 'vim.lsp.config["rust-analyzer"].settings changed after adapter activation'
  end

  local current_global_copy = copy(current_global)
  local current_server_copy = copy(current_server)
  if state.original_auto_attach_present then
    current_server_copy.auto_attach = copy(state.original_auto_attach)
  else
    current_server_copy.auto_attach = nil
  end
  current_global_copy.server = current_server_copy
  local current_entry_copy = copy(current_entry)
  if state.original_lsp_settings_present then
    current_entry_copy.settings = copy(state.original_lsp_settings)
  else
    current_entry_copy.settings = nil
  end

  -- Preserve any personal keys added to vim.g after apply and any unrelated
  -- public LSP config fields added while the adapter was active.
  vim.g.rustaceanvim = current_global_copy
  -- Neovim 0.12's public config proxy validates every assignment as a table;
  -- keep an empty public entry when there was no previous config object, with
  -- the settings field itself restored to absent.
  vim.lsp.config['rust-analyzer'] = current_entry_copy
  state.active = false
  return true
end

function M.status()
  if not state then
    return { active = false, applied = false }
  end
  return {
    active = state.active,
    applied = true,
    root = state.root,
    profile = state.profile,
    cargo = state.cargo,
    last_refusal = copy(state.last_refusal),
  }
end

return M
