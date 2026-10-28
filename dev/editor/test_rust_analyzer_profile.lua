local module_path = debug.getinfo(1, 'S').source:sub(2)
local repository_root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(module_path)))
local profile = dofile(vim.fs.joinpath(repository_root, 'dev', 'editor', 'rust_analyzer_profile.lua'))
local failures = 0

local function check(condition, message)
  if not condition then
    failures = failures + 1
    io.stderr:write('FAIL: ' .. message .. '\n')
  end
end

local function equal(left, right)
  return vim.deep_equal(left, right)
end

local root = vim.uv.fs_realpath(repository_root)
assert(root, 'repository root must resolve')
local original_cwd = vim.uv.cwd()
local cargo = vim.fn.exepath('cargo')
assert(cargo ~= '', 'cargo must resolve')
local owned_buffers = {}
local owned_foreign_root
local foreign_root
local foreign_buf
local nested_package_root = vim.fs.joinpath(root, 'tools', 'cursor_effects_smoke')
local nested_buf

local function setup_assert(condition, message)
  if not condition then
    error('SETUP ERROR: ' .. message, 0)
  end
end

local function write_fixture_file(path, lines)
  local parent = vim.fs.dirname(path)
  if vim.fn.isdirectory(parent) ~= 1 then
    setup_assert(vim.fn.mkdir(parent, 'p') == 1, 'could not create fixture directory: ' .. parent)
  end
  setup_assert(vim.fn.writefile(lines, path) == 0, 'could not write fixture file: ' .. path)
end

local function create_foreign_fixture()
  local path = vim.fn.tempname()
  setup_assert(path ~= '' and vim.fn.getftype(path) == '', 'temporary foreign project path is not unused')
  local created, create_error = vim.uv.fs_mkdir(path, 448)
  setup_assert(created, 'could not create unique foreign project directory: ' .. tostring(create_error))
  owned_foreign_root = path
  foreign_root = vim.uv.fs_realpath(path)
  setup_assert(foreign_root ~= nil, 'foreign project path could not be canonicalized')
  write_fixture_file(vim.fs.joinpath(foreign_root, 'Cargo.toml'), {
    '[package]',
    'name = "multi_launcher_m2a_foreign_fixture"',
    'version = "0.1.0"',
    'edition = "2024"',
    '',
    '[workspace]',
  })
  write_fixture_file(vim.fs.joinpath(foreign_root, 'src', 'lib.rs'), { 'pub fn foreign_fixture() {}' })
  foreign_buf = vim.api.nvim_create_buf(true, false)
  setup_assert(foreign_buf > 0, 'could not create foreign normal-file buffer')
  owned_buffers[#owned_buffers + 1] = foreign_buf
  local ok, err = pcall(vim.api.nvim_buf_set_name, foreign_buf, vim.fs.joinpath(foreign_root, 'src', 'lib.rs'))
  setup_assert(ok, 'could not name foreign normal-file buffer: ' .. tostring(err))
  setup_assert(vim.bo[foreign_buf].buftype == '', 'foreign positive-control buffer is not a normal file buffer')

  local nested_file = vim.fs.joinpath(nested_package_root, 'src', 'main.rs')
  setup_assert(vim.fn.filereadable(vim.fs.joinpath(nested_package_root, 'Cargo.toml')) == 1,
    'nested Cargo refusal fixture manifest is missing')
  setup_assert(vim.fn.filereadable(nested_file) == 1, 'nested Cargo refusal fixture source is missing')
  nested_buf = vim.api.nvim_create_buf(true, false)
  setup_assert(nested_buf > 0, 'could not create nested Cargo normal-file buffer')
  owned_buffers[#owned_buffers + 1] = nested_buf
  ok, err = pcall(vim.api.nvim_buf_set_name, nested_buf, nested_file)
  setup_assert(ok, 'could not name nested Cargo normal-file buffer: ' .. tostring(err))
  setup_assert(vim.bo[nested_buf].buftype == '', 'nested-Cargo buffer is not a normal file buffer')
end

local function create_normal_buffer(filename)
  setup_assert(vim.fn.filereadable(filename) == 1, 'normal file fixture does not exist: ' .. filename)
  local bufnr = vim.api.nvim_create_buf(true, false)
  setup_assert(bufnr > 0, 'could not create normal file buffer for ' .. filename)
  owned_buffers[#owned_buffers + 1] = bufnr
  local ok, err = pcall(vim.api.nvim_buf_set_name, bufnr, filename)
  setup_assert(ok, 'could not name normal file buffer ' .. filename .. ': ' .. tostring(err))
  setup_assert(vim.bo[bufnr].buftype == '', 'expected a normal file buffer for ' .. filename)
  return bufnr
end

local function create_scratch_buffer(filename)
  local bufnr = vim.api.nvim_create_buf(false, true)
  setup_assert(bufnr > 0, 'could not create nofile scratch buffer')
  owned_buffers[#owned_buffers + 1] = bufnr
  local ok, err = pcall(vim.api.nvim_buf_set_name, bufnr, filename)
  setup_assert(ok, 'could not name nofile scratch buffer: ' .. tostring(err))
  setup_assert(vim.bo[bufnr].buftype == 'nofile', 'scratch negative control is not nofile')
  return bufnr
end

local function delete_buffer(bufnr)
  if bufnr and vim.api.nvim_buf_is_valid(bufnr) then
    pcall(vim.api.nvim_buf_delete, bufnr, { force = true })
  end
end

local alternate_case = root:gsub('/', '\\'):gsub('%a', function(char)
  return char == char:lower() and char:upper() or char:lower()
end)
check(profile.same_root(root, alternate_case), 'Windows root normalization is case insensitive')
check(profile.path_is_within(vim.fs.joinpath(root, 'src', 'lib.rs'), root), 'repository file is within root')
check(not profile.path_is_within(root .. '-sibling/src/lib.rs', root), 'sibling prefix is outside root')

local light = profile.command_args(root, cargo, 'light')
local expected_light = {
  cargo,
  'check', '--manifest-path', vim.fs.joinpath(root, 'Cargo.toml'),
  '-p', 'multi_launcher', '--lib', '--bin', 'multi_launcher', '--message-format=json',
}
check(equal(light, expected_light), 'light profile selects only the application library and binary')
local full = profile.command_args(root, cargo, 'full')
local expected_full = {
  cargo,
  'check', '--manifest-path', vim.fs.joinpath(root, 'Cargo.toml'),
  '--workspace', '--all-targets', '--message-format=json',
}
check(equal(full, expected_full), 'full profile selects workspace and all targets without all-features')

local inherited = {
  ['rust-analyzer'] = {
    check = { enable = false, command = 'clippy', workspace = false },
    cargo = { allFeatures = false, loadOutDirsFromCheck = false },
    diagnostics = { enable = false },
    procMacro = { enable = false },
  },
}
local merged = profile.merge_profile(inherited, root, cargo, 'light')['rust-analyzer']
check(merged.check.enable == nil, 'obsolete check.enable=false is removed')
check(merged.check.command == 'check', 'profile selects Cargo check')
check(merged.check.workspace == false, 'unrelated inherited check settings are retained')
check(merged.cargo.loadOutDirsFromCheck == nil, 'obsolete check output setting is removed')
check(merged.cargo.buildScripts.enable == true, 'build-script analysis remains enabled')
check(merged.procMacro.enable == true, 'procedural macro analysis remains enabled')
check(merged.diagnostics.enable == true, 'diagnostics remain enabled')
check(merged.checkOnSave == true and merged.cfg.setTest == true, 'on-save and test semantic analysis remain enabled')

local old_global = vim.g.rustaceanvim
local old_public = vim.lsp.config['rust-analyzer']
local old_internal = package.loaded['rustaceanvim.config.internal']
local old_adapter = package.loaded['multi_launcher.rust_analyzer_profile']

local function clear_commands()
  for _, name in ipairs({
    'MultiLauncherRustAnalyzerProfile',
    'MultiLauncherRustAnalyzerRevert',
    'MultiLauncherRustAnalyzerStatus',
  }) do
    pcall(vim.api.nvim_del_user_command, name)
  end
end

local function reset_state(global_value, public_value)
  clear_commands()
  package.loaded['rustaceanvim.config.internal'] = nil
  package.loaded['multi_launcher.rust_analyzer_profile'] = profile
  vim.g.rustaceanvim = global_value
  vim.lsp.config['rust-analyzer'] = public_value or {}
end

local function scenario_absent_public_settings()
  reset_state({
    server = {
      auto_attach = false,
      settings = {
        ['rust-analyzer'] = {
          check = { enable = false },
          diagnostics = { enable = false },
          procMacro = { enable = false },
          cargo = { allFeatures = false, loadOutDirsFromCheck = false },
        },
      },
    },
    before_apply = { preserve = 'before' },
  }, { capabilities = { marker = 'existing-public-field' } })

  local ok, message = profile.apply(root)
  check(ok, 'adapter applies before the plugin caches its configuration: ' .. tostring(message))
  if not ok then
    return
  end
  local hook = vim.g.rustaceanvim.server.auto_attach
  local settings = vim.lsp.config['rust-analyzer'].settings
  check(type(hook) == 'function' and type(settings) == 'function', 'supported hook and public settings function are installed')

  -- Model the actual personal Lazy config merge, which runs after explicit
  -- adapter apply and supplies no_checks while preserving adapter auto_attach.
  local personal_no_checks = {
    ['rust-analyzer'] = {
      check = { enable = false },
      diagnostics = { enable = false },
      procMacro = { enable = false },
      cargo = { allFeatures = false, loadOutDirsFromCheck = false },
    },
  }
  vim.g.rustaceanvim = vim.tbl_deep_extend('force', vim.g.rustaceanvim, {
    dap = { personal_after_apply = true },
    server = { settings = personal_no_checks, personal_after_apply = true },
    personal_after_apply = { retained = true },
  })

  local matching = settings(root, {})['rust-analyzer']
  local foreign_settings = settings(foreign_root, {})['rust-analyzer']
  local nested_settings = settings(nested_package_root, {})['rust-analyzer']
  check(matching.diagnostics.enable == true and matching.procMacro.enable == true, 'effective matching-root settings override no_checks')
  check(matching.check.enable == nil and matching.cargo.loadOutDirsFromCheck == nil, 'effective settings remove conflicting inherited no_checks keys')
  check(matching.check.overrideCommand[1] == cargo, 'actual settings carry resolved Cargo executable')
  check(foreign_settings.diagnostics.enable == false and foreign_settings.procMacro.enable == false, 'foreign-root settings retain personal no_checks behavior')
  check(foreign_settings.check.overrideCommand == nil, 'foreign root receives no Multi Launcher command override')
  check(nested_settings.diagnostics.enable == false and nested_settings.procMacro.enable == false,
    'nested Cargo project retains personal no_checks behavior')
  check(nested_settings.check.overrideCommand == nil, 'nested Cargo project receives no Multi Launcher command override')

  check(hook(foreign_buf) == false, 'foreign Rust buffer is refused before Rustaceanvim start')
  local refusal = profile.status().last_refusal
  check(refusal ~= nil and refusal.reason == 'different-cargo-root', 'foreign-root refusal is visible to the observer')
  check(hook(nested_buf) == false, 'unlisted nested Cargo package is refused before Rustaceanvim start')
  refusal = profile.status().last_refusal
  check(refusal ~= nil and refusal.reason == 'unlisted-nested-cargo-root', 'nested-Cargo refusal is distinguishable')
  local matching_buf = create_normal_buffer(vim.fs.joinpath(root, 'src', 'lib.rs'))
  check(hook(matching_buf) == false, 'captured prior false auto_attach behavior composes on a matching normal file')
  delete_buffer(matching_buf)

  -- Add user-owned fields after apply, then ensure reversion restores only the
  -- fields it owns and the cached hook delegates to the captured false value.
  local current_global = vim.g.rustaceanvim
  current_global.server.settings.personal_after_apply = { untouched = true }
  vim.g.rustaceanvim = current_global
  local revert_ok, revert_error = profile.revert()
  check(revert_ok, 'adapter reverts without a live rust-analyzer client: ' .. tostring(revert_error))
  if revert_ok then
    local reverted_global = vim.g.rustaceanvim
    local reverted_public = vim.lsp.config['rust-analyzer']
    check(reverted_global.server.auto_attach == false, 'prior auto_attach value is restored')
    check(reverted_global.personal_after_apply.retained == true, 'personal top-level fields added later are preserved')
    check(reverted_global.server.personal_after_apply == true, 'personal server fields added later are preserved')
    check(reverted_global.server.settings.personal_after_apply.untouched == true, 'personal settings fields added later are preserved')
    check(reverted_global.dap.personal_after_apply == true, 'personal DAP fields added later are preserved')
    check(reverted_public.capabilities.marker == 'existing-public-field', 'unrelated public LSP fields are preserved')
    check(rawget(reverted_public, 'settings') == nil, 'previously absent public settings field is restored as absent')
    check(hook(foreign_buf) == false, 'cached guard delegates to prior behavior after reversion')
    check(hook(nested_buf) == false, 'cached guard delegates for nested Cargo buffer after reversion')
    check(profile.status().active == false, 'reverted cached hook is marked inactive')
    check(reverted_global.server.settings['rust-analyzer'].diagnostics.enable == false, 'effective personal settings remain after reversion')
  end
end

local function scenario_public_function_settings()
  local called_root
  local original_settings = function(project_root, defaults)
    called_root = project_root
    return {
      ['rust-analyzer'] = {
        completion = { callable = { snippets = 'personal-function' } },
      },
    }
  end
  reset_state({ server = { auto_attach = false, settings = { ['rust-analyzer'] = { diagnostics = { enable = false } } } } }, {
    completion = { marker = 'preserve' },
    settings = original_settings,
  })
  local ok, message = profile.apply(root)
  check(ok, 'adapter composes an existing public settings function: ' .. tostring(message))
  if not ok then
    return
  end
  local active_settings = vim.lsp.config['rust-analyzer'].settings
  local foreign_settings = active_settings(foreign_root, {})['rust-analyzer']
  check(called_root == foreign_root, 'prior settings function receives the owned foreign root')
  check(foreign_settings.completion.callable.snippets == 'personal-function', 'foreign root preserves the prior function result')
  check(foreign_settings.check == nil, 'foreign root gets no adapter check command')
  local matching = active_settings(root, {})['rust-analyzer']
  check(matching.completion.callable.snippets == 'personal-function', 'matching root keeps prior function fields')
  check(matching.check.overrideCommand[1] == cargo, 'matching root overlays the explicit Cargo command')
  local revert_ok, revert_error = profile.revert()
  check(revert_ok, 'settings-function composition can be reverted: ' .. tostring(revert_error))
  if revert_ok then
    check(vim.lsp.config['rust-analyzer'].settings == original_settings, 'original settings function identity is restored')
  end
end

local function scenario_absent_auto_attach_default_delegation()
  reset_state({ server = { settings = {} }, personal = { stable = true } }, nil)
  local ok, message = profile.apply(root)
  check(ok, 'adapter supports an absent prior auto_attach field: ' .. tostring(message))
  if not ok then
    return
  end
  local hook = vim.g.rustaceanvim.server.auto_attach
  local source_buf = create_normal_buffer(vim.fs.joinpath(root, 'src', 'lib.rs'))
  local core_buf = create_normal_buffer(vim.fs.joinpath(root, 'crates', 'multi_launcher_table_core', 'src', 'lib.rs'))
  local scratch_buf = create_scratch_buffer(vim.fs.joinpath(root, 'src', 'main.rs'))
  check(hook(foreign_buf) == false, 'absent-hook adapter refuses the foreign Cargo root')
  check(hook(nested_buf) == false, 'absent-hook adapter refuses the unlisted nested Cargo package')
  local expected_default = vim.fn.executable('rust-analyzer') == 1
  setup_assert(expected_default, 'rust-analyzer must resolve for the default positive-delegation fixture')
  check(vim.bo[source_buf].buftype == '' and hook(source_buf) == expected_default,
    'real root source buffer delegates to Rustaceanvim default before startup')
  check(vim.bo[core_buf].buftype == '' and hook(core_buf) == expected_default,
    'real declared table-core member buffer delegates to Rustaceanvim default before startup')
  check(vim.bo[scratch_buf].buftype == 'nofile' and hook(scratch_buf) == false,
    'scratch/nofile buffer remains refused before startup')
  local revert_ok, revert_error = profile.revert()
  check(revert_ok, 'absent auto_attach scenario reverts: ' .. tostring(revert_error))
  if revert_ok then
    check(rawget(vim.g.rustaceanvim.server, 'auto_attach') == nil, 'previously absent auto_attach field is restored as absent')
    check(hook(source_buf) == expected_default, 'cached hook delegates for root normal file after reversion')
    check(hook(core_buf) == expected_default, 'cached hook delegates for member normal file after reversion')
    check(hook(scratch_buf) == false, 'cached hook still refuses scratch/nofile buffer after reversion')
    check(vim.g.rustaceanvim.personal.stable, 'preexisting personal fields survive absent-hook reversion')
  end
  delete_buffer(source_buf)
  delete_buffer(core_buf)
  delete_buffer(scratch_buf)
end

local function scenario_function_auto_attach_composition()
  local calls = {}
  local original_auto_attach = function(bufnr)
    calls[#calls + 1] = bufnr
    return vim.api.nvim_buf_is_valid(bufnr) and vim.bo[bufnr].buftype == ''
  end
  reset_state({ server = { auto_attach = original_auto_attach, settings = {} } }, nil)
  local ok, message = profile.apply(root)
  check(ok, 'adapter captures a function-valued auto_attach hook: ' .. tostring(message))
  if not ok then
    return
  end
  local hook = vim.g.rustaceanvim.server.auto_attach
  local source_buf = create_normal_buffer(vim.fs.joinpath(root, 'src', 'lib.rs'))
  local core_buf = create_normal_buffer(vim.fs.joinpath(root, 'crates', 'multi_launcher_table_core', 'src', 'lib.rs'))
  local before_refusal_calls = #calls
  check(hook(foreign_buf) == false and #calls == before_refusal_calls,
    'foreign buffer is rejected before captured function delegation')
  check(hook(nested_buf) == false and #calls == before_refusal_calls,
    'nested Cargo buffer is rejected before captured function delegation')
  check(hook(source_buf) == true and calls[#calls] == source_buf,
    'matching root normal file delegates to captured function')
  check(hook(core_buf) == true and calls[#calls] == core_buf,
    'declared member normal file delegates to captured function')

  local revert_ok, revert_error = profile.revert()
  check(revert_ok, 'function-valued auto_attach scenario reverts: ' .. tostring(revert_error))
  if revert_ok then
    check(vim.g.rustaceanvim.server.auto_attach == original_auto_attach,
      'original auto_attach function identity is restored')
    local before_cached_delegation = #calls
    check(hook(source_buf) == true and #calls == before_cached_delegation + 1,
      'cached hook delegates to original function after reversion')
  end
  delete_buffer(source_buf)
  delete_buffer(core_buf)
end

local function precondition_refusals()
  local before_global = vim.g.rustaceanvim
  local before_public = vim.lsp.config['rust-analyzer']
  local before_cwd = vim.uv.cwd()
  local changed_cwd, cwd_error = pcall(vim.cmd.cd, foreign_root)
  setup_assert(changed_cwd, 'could not enter the owned foreign project: ' .. tostring(cwd_error))
  local checks_ok, checks_error = xpcall(function()
    local ok, reason = profile.apply(root)
    check(not ok and type(reason) == 'string' and reason:find('working directory', 1, true) ~= nil,
      'owned foreign project CWD refuses adapter activation: ' .. tostring(reason) .. ' (cwd=' .. tostring(vim.uv.cwd()) .. ')')
    check(equal(vim.g.rustaceanvim, before_global), 'foreign-root refusal leaves global settings unchanged')
    check(equal(vim.lsp.config['rust-analyzer'], before_public), 'foreign-root refusal leaves public LSP config unchanged')
  end, debug.traceback)
  local restored_cwd, restore_error = pcall(vim.cmd.cd, before_cwd)
  setup_assert(restored_cwd, 'could not restore CWD after foreign-root refusal: ' .. tostring(restore_error))
  if not checks_ok then
    error(checks_error, 0)
  end

  package.loaded['rustaceanvim.config.internal'] = {}
  local ok, reason = profile.apply(root)
  check(not ok and reason:find('already cached', 1, true) ~= nil, 'late activation refuses cached Rustaceanvim config')
  package.loaded['rustaceanvim.config.internal'] = nil
end

local ok, err = xpcall(function()
  vim.cmd.cd(root)
  create_foreign_fixture()
  precondition_refusals()
  scenario_absent_public_settings()
  scenario_public_function_settings()
  scenario_absent_auto_attach_default_delegation()
  scenario_function_auto_attach_composition()
end, debug.traceback)

local cleanup_errors = {}
local cwd_restored, cwd_restore_error = pcall(vim.cmd.cd, original_cwd)
if not cwd_restored then
  cleanup_errors[#cleanup_errors + 1] = 'could not restore original CWD: ' .. tostring(cwd_restore_error)
end
clear_commands()
package.loaded['rustaceanvim.config.internal'] = old_internal
package.loaded['multi_launcher.rust_analyzer_profile'] = old_adapter
vim.g.rustaceanvim = old_global
vim.lsp.config['rust-analyzer'] = old_public or {}
for _, bufnr in ipairs(owned_buffers) do
  if vim.api.nvim_buf_is_valid(bufnr) then
    local deleted, delete_error = pcall(vim.api.nvim_buf_delete, bufnr, { force = true })
    if not deleted then
      cleanup_errors[#cleanup_errors + 1] = ('could not delete owned buffer %d: %s'):format(bufnr, tostring(delete_error))
    end
  end
end
if owned_foreign_root then
  local deleted = vim.fn.delete(owned_foreign_root, 'rf')
  if deleted ~= 0 then
    cleanup_errors[#cleanup_errors + 1] = 'could not delete owned foreign project: ' .. owned_foreign_root
  end
end

if not ok or #cleanup_errors > 0 then
  if not ok then
    io.stderr:write(err .. '\n')
  end
  if #cleanup_errors > 0 then
    io.stderr:write('SETUP ERROR: cleanup failed: ' .. table.concat(cleanup_errors, '; ') .. '\n')
  end
  os.exit(1)
end
if failures > 0 then
  io.stderr:write(('FAIL: %d rust-analyzer adapter assertion(s) failed\n'):format(failures))
  os.exit(1)
end
print('PASS: rust-analyzer adapter root, profile, composition, refusal, and reversion checks')
