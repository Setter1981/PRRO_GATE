using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class CheckShift
{
	private string[,] Check;

	private int CheckS;

	public int ChecksShift => CheckS;

	public string Seller
	{
		get
		{
			if (x < 0 || x > 1)
			{
				return "";
			}
			if ((y < 1) | (y > CheckS))
			{
				return "";
			}
			return Check[x, y];
		}
	}

	public CheckShift(string eShift)
	{
		Check = new string[2, 1];
		CheckS = 0;
		Check = new string[2, checked(CheckS + 1)];
		LoadCheckAll(eShift);
	}

	private void LoadCheckAll(string eShift)
	{
		checked
		{
			try
			{
				string connection = All.A.Connection;
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select checkidficscal from ksef where shiftid =" + eShift;
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					CheckS++;
					ref string[,] check = ref Check;
					check = (string[,])Utils.CopyArray((Array)check, (Array)new string[2, CheckS + 1]);
					Check[0, CheckS] = sQLiteDataReader[0].ToString();
					Check[1, CheckS] = "";
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				CheckS = 0;
				Check = new string[2, CheckS + 1];
				ProjectData.ClearProjectError();
			}
		}
	}
}
