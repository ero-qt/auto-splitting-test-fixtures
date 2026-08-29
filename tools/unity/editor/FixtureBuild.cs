// The oldest editors compile this with their C# 4.0 compiler and report a
// build as an error string rather than a report, so the file stays inside
// what every supported editor accepts.
using System;
using System.IO;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine.SceneManagement;
#if UNITY_2018_1_OR_NEWER
using UnityEditor.Build.Reporting;
#endif

public static class FixtureBuild
{
    /// <summary>
    ///     Builds one fixture player, pointed here through <c>-executeMethod</c>.
    ///     The <c>unity-fixtures</c> tool copies this file into the throwaway project, then starts the editor in batch mode.
    /// </summary>
    /// <remarks>
    ///     <c>-fixtureOut</c> says where the player goes and <c>-fixtureBackend</c> picks Mono or IL2CPP;
    ///     Unity's own <c>-buildTarget</c> selects the platform.
    ///     The editor writes that command line into its log to document how a player was built.
    /// </remarks>
    public static void Build()
    {
        string outDir = ArgAfter("-fixtureOut");
        string backend = ArgAfter("-fixtureBackend");

        var target = EditorUserBuildSettings.activeBuildTarget;
        var group = BuildPipeline.GetBuildTargetGroup(target);
        PlayerSettings.SetScriptingBackend(
            group,
            backend == "il2cpp" ? ScriptingImplementation.IL2CPP : ScriptingImplementation.Mono2x);

        var options = new BuildPlayerOptions
        {
            scenes = new[] { EmptyScene() },
            locationPathName = Path.Combine(outDir, PlayerName(target)),
            target = target,
        };

#if UNITY_2018_1_OR_NEWER
        var report = BuildPipeline.BuildPlayer(options);
        EditorApplication.Exit(report.summary.result == BuildResult.Succeeded ? 0 : 1);
#else
        string error = BuildPipeline.BuildPlayer(options);
        EditorApplication.Exit(string.IsNullOrEmpty(error) ? 0 : 1);
#endif
    }

    // BuildPlayer takes scene paths, and a scene has a path once it is
    // saved. This saves one empty scene and hands back where it went.
    static string EmptyScene()
    {
        const string path = "Assets/Fixture.unity";
        if (!File.Exists(path))
        {
            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            EditorSceneManager.SaveScene(scene, path);
        }

        return path;
    }

    static string PlayerName(BuildTarget target)
    {
        switch (target)
        {
#if UNITY_2017_3_OR_NEWER
            case BuildTarget.StandaloneOSX:
#else
            case BuildTarget.StandaloneOSXUniversal:
#endif
                return "fixture.app";
            case BuildTarget.StandaloneLinux64:
                return "fixture";
            default:
                return "fixture.exe";
        }
    }

    static string ArgAfter(string name)
    {
        string[] args = Environment.GetCommandLineArgs();
        int at = Array.IndexOf(args, name);
        if (at < 0 || at + 1 >= args.Length)
        {
            throw new ArgumentException("missing " + name);
        }

        return args[at + 1];
    }
}
