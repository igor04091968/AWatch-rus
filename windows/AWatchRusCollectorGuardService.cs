using System;
using System.Diagnostics;
using System.IO;
using System.ServiceProcess;
using System.Threading;

namespace AWatchRus
{
    public sealed class CollectorGuardService : ServiceBase
    {
        private Process child;
        private readonly ServiceOptions options;
        private readonly object sync = new object();
        private bool stopping;
        private string childFileName;
        private string childArguments;
        private DateTime restartWindowStartedUtc = DateTime.UtcNow;
        private int restartCountInWindow;

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
            Log("service starting");

            childFileName = string.IsNullOrWhiteSpace(options.ExecPath) ? options.PowerShellPath : options.ExecPath;
            childArguments = string.IsNullOrWhiteSpace(options.ExecPath)
                ? string.Format(
                    "-NoProfile -ExecutionPolicy Bypass -File \"{0}\" -ConfigPath \"{1}\" -Mode {2} -LoopSeconds {3}",
                    options.ScriptPath,
                    options.ConfigPath,
                    options.Mode,
                    options.LoopSeconds)
                : (options.ExecArgs ?? string.Empty);

            StartChild("initial start");
        }

        private void StartChild(string reason)
        {
            lock (sync)
            {
                if (stopping)
                {
                    return;
                }
                if (child != null && !child.HasExited)
                {
                    return;
                }
            }

            var psi = new ProcessStartInfo
            {
                FileName = childFileName,
                Arguments = childArguments,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = false,
                RedirectStandardError = false,
            };
            var process = new Process
            {
                StartInfo = psi,
                EnableRaisingEvents = true,
            };
            process.Exited += ChildExited;
            process.Start();
            lock (sync)
            {
                child = process;
            }
            Log("child pid=" + process.Id + " exec=" + childFileName + " reason=" + reason);
        }

        private void ChildExited(object sender, EventArgs args)
        {
            var process = sender as Process;
            var exitCode = "unknown";
            try
            {
                if (process != null)
                {
                    exitCode = process.ExitCode.ToString();
                }
            }
            catch
            {
            }

            lock (sync)
            {
                if (stopping)
                {
                    Log("child exited during stop exitCode=" + exitCode);
                    return;
                }
            }

            Log("child exited unexpectedly exitCode=" + exitCode);
            if (!RegisterChildRestart())
            {
                Log("child restart budget exhausted; exiting service for SCM recovery");
                Environment.Exit(1);
                return;
            }

            var restartThread = new Thread(new ThreadStart(delegate
            {
                Thread.Sleep(Math.Max(1, options.ChildRestartDelaySeconds) * 1000);
                StartChild("child-exit restart");
            }));
            restartThread.IsBackground = true;
            restartThread.Start();
        }

        private bool RegisterChildRestart()
        {
            lock (sync)
            {
                var now = DateTime.UtcNow;
                if ((now - restartWindowStartedUtc).TotalSeconds > options.ChildRestartWindowSeconds)
                {
                    restartWindowStartedUtc = now;
                    restartCountInWindow = 0;
                }
                restartCountInWindow++;
                Log("child restart budget count=" + restartCountInWindow + " windowSeconds=" + options.ChildRestartWindowSeconds);
                return restartCountInWindow <= options.MaxChildRestartsInWindow;
            }
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
                Log(reason);
                Process process;
                lock (sync)
                {
                    stopping = true;
                    process = child;
                }
                if (process != null && !process.HasExited)
                {
                    process.Kill();
                    process.WaitForExit(10000);
                }
            }
            catch (Exception ex)
            {
                try
                {
                    Log("stop error: " + ex.Message);
                }
                catch
                {
                }
            }
        }

        private void Log(string message)
        {
            File.AppendAllText(options.LogPath, DateTime.Now.ToString("s") + " " + message + Environment.NewLine);
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
        public string ExecPath = null;
        public string ExecArgs = null;
        public int ChildRestartDelaySeconds = 5;
        public int MaxChildRestartsInWindow = 5;
        public int ChildRestartWindowSeconds = 600;
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
                else if (key == "--exec") options.ExecPath = value;
                else if (key == "--args") options.ExecArgs = value;
                i++;
            }
            return options;
        }
    }
}
