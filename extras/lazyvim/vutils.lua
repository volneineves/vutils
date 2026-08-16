return {
  {
    "volneineves/vutils",
    build = "cargo build --release --locked",
    cmd = { "Vutils" },
    keys = {
      {
        "<leader>uv",
        function()
          require("vutils").open()
        end,
        desc = "Open vutils TUI",
      },
    },
    opts = {
      keymap = false,
    },
  },
}
