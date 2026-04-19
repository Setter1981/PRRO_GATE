using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class LogSaveTextRobot
{
	private string PF;

	public string FNlog;

	public bool LogOn;

	private string Connection;

	private string Fn;

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

	public LogSaveTextRobot(string ConnectionS, string fnS)
	{
		PF = "";
		FNlog = "";
		LogOn = true;
		Connection = ConnectionS;
		Fn = fnS;
	}

	public void SaveTextToLog(string NameSLog, string TextIn, string TextOut = "")
	{
		if (!LogOn)
		{
			return;
		}
		string text = "";
		string value = string.Concat(str1: (!OfflineTrue()) ? ("   robot online  " + Fn) : ("  robot offline  " + Fn), str0: DateTime.Now.ToString());
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

	internal bool OfflineTrue()
	{
		bool result;
		if (Operators.CompareString(Fn, "7000000512", false) == 0)
		{
			result = true;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT ID FROM ksef WHERE offline > '1'";
				result = (sQLiteCommand.ExecuteReader().Read() ? true : false);
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}
}
