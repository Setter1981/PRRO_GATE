using System;
using System.IO;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[StandardModule]
internal sealed class All
{
	public static ClassFiscal WC = new ClassFiscal();

	public static bool ProgramDataFolder = true;

	public static string FileN = "";

	public const string Proga = "WebCheck";

	public static WebCheck.IniHGB f = new WebCheck.IniHGB(MyDoc() + "\\WebCheck\\settings.ini");

	public static WebCheck.SQLlite l = new WebCheck.SQLlite();

	public static string FN = "";

	public static bool status = false;

	internal static string MyDoc()
	{
		if (ProgramDataFolder)
		{
			return Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData);
		}
		return Environment.GetFolderPath(Environment.SpecialFolder.Personal);
	}

	internal static void NewFolder()
	{
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\");
		}
	}
}
