using System;
using System.Diagnostics;
using System.IO;
using System.ServiceProcess;

namespace AWatchRus
{
    public sealed class CollectorGuardService : ServiceBase
    {
        private Process child;
        private readonly ServiceOptions options;

        public CollectorGuardService(ServiceOptions options)
        {
            this.options = options;
            ServiceName = options.ServiceName;
            CanStop = true;
            CanShutdown = true;
        }

        protected override void OnStart(string[] args)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(options.LogPath));
            File.AppendAllText(options.LogPath, DateTime.Now.ToString("s") + " service starting" + Environment.NewLine);

            var psi = new ProcessStartInfo
            {
                FileName = options.PowerShellPath,
                Arguments = string.Format(
                    "-NoProfile -ExecutionPolicy Bypass -File \"{0}\" -ConfigPath \"{1}\" -Mode {2} -LoopSeconds {3}",
                    options.ScriptPath,
                    options.ConfigPath,
                    options.Mode,
                    options.LoopSeconds),
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = false,
                RedirectStandardError = false,
            };
            child = Process.Start(psi);
            File.AppendAllText(options.LogPath, DateTime.Now.ToString("s") + " child pid=" + child.Id + Environment.NewLine);
        }

        protected override void OnStop()
        {
            StopChild("service stopping");
        }

        protected override void OnShutdown()
        {
            StopChild("system shutdown");
        }

        private void StopChild(string reason)
        {
            try
            {
                File.AppendAllText(options.LogPath, DateTime.Now.ToString("s") + " " + reason + Environment.NewLine);
                if (child != null && !child.HasExited)
                {
                    child.Kill();
                    child.WaitForExit(10000);
                }
            }
            catch (Exception ex)
            {
                try
                {
                    File.AppendAllText(options.LogPath, DateTime.Now.ToString("s") + " stop error: " + ex.Message + Environment.NewLine);
                }
                catch
                {
                }
            }
        }
    }

    public sealed class ServiceOptions
    {
        public string ServiceName = "AWatchRusCollectorGuard";
        public string PowerShellPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows), "System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        public string ScriptPath = @"C:\Program Files\AWatch-rus\windows\aw-collector-guard.ps1";
        public string ConfigPath = @"C:\ProgramData\AWatch-rus\deployment-config.json";
        public string Mode = "shadow";
        public int LoopSeconds = 60;
        public string LogPath = @"C:\ProgramData\AWatch-rus\logs\collector-guard-service.log";
    }

    internal static class Program
    {
        private static void Main(string[] args)
        {
            var options = Parse(args);
            ServiceBase.Run(new CollectorGuardService(options));
        }

        private static ServiceOptions Parse(string[] args)
        {
            var options = new ServiceOptions();
            for (var i = 0; i < args.Length; i++)
            {
                var key = args[i].ToLowerInvariant();
                var value = i + 1 < args.Length ? args[i + 1] : null;
                if (value == null || value.StartsWith("--", StringComparison.Ordinal))
                {
                    continue;
                }
                if (key == "--service-name") options.ServiceName = value;
                else if (key == "--script") options.ScriptPath = value;
                else if (key == "--config") options.ConfigPath = value;
                else if (key == "--mode") options.Mode = value;
                else if (key == "--loop")
                {
                    int parsed;
                    if (int.TryParse(value, out parsed)) options.LoopSeconds = parsed;
                }
                else if (key == "--log") options.LogPath = value;
                i++;
            }
            return options;
        }
    }
}
