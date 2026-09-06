using System;
using System.Diagnostics;
using System.IO;

namespace Tickwise.Tests
{
    /// <summary>
    /// Runs the tickwise command line tool from the repository's Cargo
    /// workspace, so the recordings these tests write are verified by the
    /// same binary users run. The wrapper has no .rec reader of its own.
    /// </summary>
    internal static class TickwiseCli
    {
        public sealed class Result
        {
            public int ExitCode;
            public string Stdout;
            public string Stderr;
        }

        /// <summary>
        /// The repository root: TICKWISE_REPO_ROOT when set, otherwise found
        /// by walking up from the test assembly until the directory holding
        /// both Cargo.toml and bridges/ appears.
        /// </summary>
        public static string RepoRoot()
        {
            string overridden = Environment.GetEnvironmentVariable("TICKWISE_REPO_ROOT");
            if (!string.IsNullOrEmpty(overridden))
            {
                return overridden;
            }
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            while (dir != null)
            {
                if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml"))
                    && Directory.Exists(Path.Combine(dir.FullName, "bridges")))
                {
                    return dir.FullName;
                }
                dir = dir.Parent;
            }
            throw new InvalidOperationException(
                "could not find the Tickwise repository root above " + AppContext.BaseDirectory);
        }

        public static Result Run(params string[] args)
        {
            var start = new ProcessStartInfo
            {
                FileName = "cargo",
                WorkingDirectory = RepoRoot(),
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                // The CLI writes UTF-8 regardless of the console code page.
                StandardOutputEncoding = System.Text.Encoding.UTF8,
                StandardErrorEncoding = System.Text.Encoding.UTF8,
                UseShellExecute = false,
            };
            start.ArgumentList.Add("run");
            start.ArgumentList.Add("-q");
            start.ArgumentList.Add("-p");
            start.ArgumentList.Add("tickwise-cli");
            start.ArgumentList.Add("--");
            foreach (string arg in args)
            {
                start.ArgumentList.Add(arg);
            }
            start.Environment["NO_COLOR"] = "1";

            using var process = Process.Start(start);
            string stdout = process.StandardOutput.ReadToEnd();
            string stderr = process.StandardError.ReadToEnd();
            process.WaitForExit();
            return new Result { ExitCode = process.ExitCode, Stdout = stdout, Stderr = stderr };
        }
    }
}
