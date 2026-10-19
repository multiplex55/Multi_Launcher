param(
    [Parameter(Mandatory = $true)]
    [string] $Nonce
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

Add-Type -ReferencedAssemblies System.Windows.Forms,System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public static class PinboardLiveSmokeNative
{
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetProcessDpiAwarenessContext(IntPtr context);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("dwmapi.dll", PreserveSig = true)]
    public static extern int DwmGetWindowAttribute(IntPtr hwnd, int attribute, out Rect value, uint size);

    [DllImport("dwmapi.dll", PreserveSig = true)]
    public static extern int DwmFlush();

    public static uint RectSize { get { return (uint)Marshal.SizeOf(typeof(Rect)); } }
}

public sealed class PinboardLiveSmokeSource : Form
{
    private char phase = 'A';

    public PinboardLiveSmokeSource()
    {
        FormBorderStyle = FormBorderStyle.None;
        AutoScaleMode = AutoScaleMode.None;
        StartPosition = FormStartPosition.Manual;
        TopMost = true;
        Size = new Size(360, 240);
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint, true);
    }

    protected override bool ShowWithoutActivation { get { return true; } }

    public void SetPhase(char next)
    {
        phase = next;
        Refresh();
        Update();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        Color background = phase == 'A' ? Color.FromArgb(19, 41, 73)
            : phase == 'B' ? Color.FromArgb(31, 83, 137)
            : Color.FromArgb(42, 94, 28);
        Color first = phase == 'A' ? Color.FromArgb(250, 210, 25)
            : phase == 'B' ? Color.FromArgb(35, 240, 70)
            : Color.FromArgb(245, 40, 170);
        Color second = phase == 'A' ? Color.FromArgb(20, 180, 245)
            : phase == 'B' ? Color.FromArgb(250, 90, 15)
            : Color.FromArgb(25, 210, 230);
        using (SolidBrush baseBrush = new SolidBrush(background))
        using (SolidBrush firstBrush = new SolidBrush(first))
        using (SolidBrush secondBrush = new SolidBrush(second))
        {
            e.Graphics.FillRectangle(baseBrush, 0, 0, 360, 240);
            // The selected 64x64 crop begins at (43, 57). These asymmetric
            // anchors make both crop origin and RGB channel order observable.
            e.Graphics.FillRectangle(firstBrush, 47, 62, 12, 12);
            e.Graphics.FillRectangle(secondBrush, 82, 84, 9, 15);
        }
        base.OnPaint(e);
    }
}

public sealed class PinboardLiveSmokeCover : Form
{
    public PinboardLiveSmokeCover(int size)
    {
        FormBorderStyle = FormBorderStyle.None;
        AutoScaleMode = AutoScaleMode.None;
        ShowInTaskbar = false;
        TopMost = true;
        BackColor = Color.Magenta;
        Size = new Size(size, size);
    }

    protected override bool ShowWithoutActivation { get { return true; } }

    protected override CreateParams CreateParams
    {
        get
        {
            CreateParams parameters = base.CreateParams;
            parameters.ExStyle |= 0x08000000; // WS_EX_NOACTIVATE
            return parameters;
        }
    }
}

public sealed class PinboardLiveSmokeDispatcher : Form
{
    private readonly PinboardLiveSmokeSource source;
    private readonly int cropX;
    private readonly int cropY;
    private readonly int cropWidth;
    private PinboardLiveSmokeCover cover;

    public PinboardLiveSmokeDispatcher(PinboardLiveSmokeSource source, int cropX, int cropY, int cropWidth)
    {
        this.source = source;
        this.cropX = cropX;
        this.cropY = cropY;
        this.cropWidth = cropWidth;
        FormBorderStyle = FormBorderStyle.None;
        AutoScaleMode = AutoScaleMode.None;
        ShowInTaskbar = false;
        Size = new Size(1, 1);
        Location = new Point(-32000, -32000);
        Opacity = 0;
    }

    protected override bool ShowWithoutActivation { get { return true; } }

    protected override CreateParams CreateParams
    {
        get
        {
            CreateParams parameters = base.CreateParams;
            parameters.ExStyle |= 0x08000000; // WS_EX_NOACTIVATE
            return parameters;
        }
    }

    public void StartReader()
    {
        var reader = new System.Threading.Thread(() =>
        {
            string line;
            while ((line = Console.ReadLine()) != null)
            {
                string command = line;
                try
                {
                    BeginInvoke(new Action(() => HandleCommand(command)));
                }
                catch (InvalidOperationException)
                {
                    return;
                }
            }
        });
        reader.IsBackground = true;
        reader.Name = "pinboard-live-smoke-stdin";
        reader.Start();
    }

    private void HandleCommand(string line)
    {
        string[] parts = line.Split(' ');
        long id;
        if (parts.Length < 2 || !long.TryParse(parts[parts.Length - 1], out id))
        {
            Console.WriteLine("ERROR|invalid command");
            Console.Out.Flush();
            return;
        }
        try
        {
            switch (parts[0])
            {
                case "PAINT":
                    if (parts.Length != 3 || parts[1].Length != 1)
                        throw new InvalidOperationException("PAINT requires one phase");
                    source.SetPhase(parts[1][0]);
                    break;
                case "MOVE":
                    source.Location = new Point(source.Left + 40, source.Top);
                    source.Refresh();
                    source.Update();
                    break;
                case "COVER":
                    if (cover == null || cover.IsDisposed)
                    {
                        cover = new PinboardLiveSmokeCover(cropWidth);
                    }
                    cover.Location = new Point(source.Left + cropX, source.Top + cropY);
                    cover.Show();
                    cover.BringToFront();
                    cover.Refresh();
                    cover.Update();
                    break;
                case "CLOSE_SOURCE":
                    if (cover != null && !cover.IsDisposed)
                        cover.Close();
                    if (!source.IsDisposed)
                        source.Close();
                    break;
                case "QUIT":
                    if (cover != null && !cover.IsDisposed)
                        cover.Close();
                    if (!source.IsDisposed)
                        source.Close();
                    break;
                default:
                    throw new InvalidOperationException("unknown command " + parts[0]);
            }
            PinboardLiveSmokeNative.DwmFlush();
            Console.WriteLine("ACK|" + id);
            Console.Out.Flush();
            if (parts[0] == "QUIT")
                Application.ExitThread();
        }
        catch (Exception error)
        {
            Console.WriteLine("ERROR|" + id + "|" + error.Message.Replace('|', '/'));
            Console.Out.Flush();
        }
    }
}
'@

try {
    # Per-monitor-v2 makes WinForms client dimensions physical pixels, matching
    # DWM extended-frame bounds and the WGC content dimensions used by the test.
    [void][PinboardLiveSmokeNative]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))
    [System.Windows.Forms.Application]::EnableVisualStyles()
    $source = [PinboardLiveSmokeSource]::new()
    $work = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    $source.Location = [System.Drawing.Point]::new($work.Right - 400, $work.Top + 80)
    $source.SetPhase('A')
    $source.Show()
    $source.BringToFront()
    $source.Refresh()
    $source.Update()

    $dispatcher = [PinboardLiveSmokeDispatcher]::new($source, 43, 57, 64)
    [void]$dispatcher.Handle
    $dispatcher.Show()
    $dispatcher.Hide()
    $dispatcher.StartReader()

    $rect = [PinboardLiveSmokeNative+Rect]::new()
    $result = [PinboardLiveSmokeNative]::DwmGetWindowAttribute(
        $source.Handle, 9, [ref]$rect, [PinboardLiveSmokeNative]::RectSize)
    if ($result -ne 0) { throw "DwmGetWindowAttribute failed: $result" }
    [uint32]$sourceProcessId = 0
    $threadId = [PinboardLiveSmokeNative]::GetWindowThreadProcessId($source.Handle, [ref]$sourceProcessId)
    [Console]::WriteLine("READY|$Nonce|$([long]$source.Handle)|$sourceProcessId|$threadId|$($rect.Left)|$($rect.Top)|$($rect.Right - $rect.Left)|$($rect.Bottom - $rect.Top)")
    [Console]::Out.Flush()
    [System.Windows.Forms.Application]::Run([System.Windows.Forms.ApplicationContext]::new())
} catch {
    [Console]::Error.WriteLine($_.Exception.ToString())
    exit 1
}
