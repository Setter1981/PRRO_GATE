using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class AllDokShefts
{
	private string[,] Op;

	private int OpS;

	public int Checks => OpS;

	public string InfaCheck
	{
		get
		{
			if (x < 0 || x > 5)
			{
				return "";
			}
			if ((y < 1) | (y > OpS))
			{
				return "";
			}
			return Op[x, y];
		}
	}

	public AllDokShefts(int sh)
	{
		Op = new string[6, 1];
		OpS = 0;
		Op = new string[6, checked(OpS + 1)];
		LoadSheftsAll(sh);
	}

	private void LoadSheftsAll(int ch)
	{
		checked
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM ksef WHERE shiftid ='" + ch + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					OpS++;
					ref string[,] op = ref Op;
					op = (string[,])Utils.CopyArray((Array)op, (Array)new string[6, OpS + 1]);
					Op[0, OpS] = sQLiteDataReader[5].ToString();
					Op[1, OpS] = sQLiteDataReader[4].ToString();
					Op[2, OpS] = All.CheckNumToTyp(sQLiteDataReader[6].ToString());
					Op[3, OpS] = sQLiteDataReader[10].ToString();
					Op[4, OpS] = sQLiteDataReader[7].ToString();
					Op[5, OpS] = sQLiteDataReader[11].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				OpS = 0;
				Op = new string[6, OpS + 1];
				ProjectData.ClearProjectError();
			}
		}
	}
}
