using System;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

internal class LogSaveText
{
	private string PF;

	public bool LogOn;

	public string PathFile
	{
		get
		{
			return PF;
		}
		set
		{
			PF = value;
		}
	}

	public LogSaveText()
	{
		LogOn = true;
	}

	internal bool LastLog()
	{
		DateTime now = DateTime.Now;
		if (now.Month == All.A.MonhtLast)
		{
			return false;
		}
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\log_old.txt";
		try
		{
			if (File.Exists(text))
			{
				FileSystem.DeleteFile(text);
				Application.DoEvents();
			}
			if (File.Exists(PF))
			{
				FileSystem.RenameFile(PF, "log_old.txt");
				Application.DoEvents();
			}
			if (File.Exists(PF))
			{
				FileSystem.DeleteFile(PF);
				Application.DoEvents();
			}
			All.A.MonhtLast = now.Month;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			All.A.MonhtLast = 0;
			ProjectData.ClearProjectError();
		}
		All.f.IntigerWriteFN(All.A.FN, "MonhtLast", All.A.MonhtLast);
		return true;
	}

	public void SaveTextToLog(string NameSLog, string TextIn, string TextOut = "")
	{
		if (!All.A.Status || !LogOn)
		{
			return;
		}
		string text = "COM";
		if (All.A.TypWork == 2019)
		{
			text = "Server DLL";
		}
		else if (All.A.TypWork == 2020)
		{
			text = "Serwer EXE";
		}
		string text2 = "";
		string value = string.Concat(str1: (!All.l.OfflineTrue()) ? ("   online  " + All.A.FN) : ("  offline  " + All.A.FN), str0: DateTime.Now.ToString(), str2: " ");
		NameSLog = NameSLog + "    v." + All.VersionDll() + "    #" + text + "#";
		try
		{
			StreamWriter streamWriter = new StreamWriter(PF, append: true);
			streamWriter.WriteLine(value);
			streamWriter.WriteLine(NameSLog);
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(TextIn);
			streamWriter.WriteLine("  ");
			if (Operators.CompareString(TextOut.Trim(), "", false) != 0)
			{
				streamWriter.WriteLine(TextOut);
			}
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(" =====================================================");
			streamWriter.WriteLine("  ");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void SaveTextToLogAll(string NameSLog, string TextIn, string TextOut = "")
	{
		string text = All.A.FN;
		if (Operators.CompareString(text.Trim(), "", false) == 0)
		{
			text = "NEMO ";
		}
		string text2 = "";
		string value = string.Concat(str1: (!All.l.OfflineTrue()) ? ("   online  " + text) : ("  offline  " + text), str0: DateTime.Now.ToString(), str2: " ");
		NameSLog = NameSLog + "    v." + All.VersionDll();
		try
		{
			StreamWriter streamWriter = new StreamWriter(PF, append: true);
			streamWriter.WriteLine(value);
			streamWriter.WriteLine(NameSLog);
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(TextIn);
			streamWriter.WriteLine("  ");
			if (Operators.CompareString(TextOut.Trim(), "", false) != 0)
			{
				streamWriter.WriteLine(TextOut);
			}
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(" =====================================================");
			streamWriter.WriteLine("  ");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void SaveTextToLogCardserv(string NameSLog, string TextIn, string TextOut = "")
	{
		string value = DateTime.Now.ToString() + " -Cardserv-";
		NameSLog = NameSLog + "    v." + All.VersionDll() + " ";
		try
		{
			StreamWriter streamWriter = new StreamWriter(PF, append: true);
			streamWriter.WriteLine(value);
			streamWriter.WriteLine(NameSLog);
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(TextIn);
			streamWriter.WriteLine("  ");
			if (Operators.CompareString(TextOut.Trim(), "", false) != 0)
			{
				streamWriter.WriteLine(TextOut);
			}
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(" =====================================================");
			streamWriter.WriteLine("  ");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}
}
