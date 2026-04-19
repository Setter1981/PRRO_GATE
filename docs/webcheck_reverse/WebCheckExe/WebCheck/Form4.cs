using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class Form4 : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ADDoperator")]
	private Button _ADDoperator;

	[field: AccessedThroughProperty("DG")]
	internal virtual DataGridView DG
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ADDoperator
	{
		[CompilerGenerated]
		get
		{
			return _ADDoperator;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ADDoperator_Click;
			Button aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click -= eventHandler;
			}
			_ADDoperator = value;
			aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("CashIS")]
	internal virtual TextBox CashIS
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PayName")]
	internal virtual TextBox PayName
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TAXPRCB")]
	internal virtual TextBox TAXPRCB
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ID")]
	internal virtual DataGridViewTextBoxColumn ID
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TAXES")]
	internal virtual DataGridViewTextBoxColumn TAXES
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("EXCISE")]
	internal virtual DataGridViewTextBoxColumn EXCISE
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TAXPRC")]
	internal virtual DataGridViewTextBoxColumn TAXPRC
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public Form4()
	{
		((Form)this).Load += Form4_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_02bb: Unknown result type (might be due to invalid IL or missing references)
		//IL_02c5: Expected O, but got Unknown
		//IL_0341: Unknown result type (might be due to invalid IL or missing references)
		//IL_034b: Expected O, but got Unknown
		//IL_03c4: Unknown result type (might be due to invalid IL or missing references)
		//IL_03ce: Expected O, but got Unknown
		//IL_04b9: Unknown result type (might be due to invalid IL or missing references)
		//IL_04c3: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(Form4));
		DG = new DataGridView();
		ID = new DataGridViewTextBoxColumn();
		TAXES = new DataGridViewTextBoxColumn();
		EXCISE = new DataGridViewTextBoxColumn();
		TAXPRC = new DataGridViewTextBoxColumn();
		ADDoperator = new Button();
		CashIS = new TextBox();
		PayName = new TextBox();
		TAXPRCB = new TextBox();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[4]
		{
			(DataGridViewColumn)ID,
			(DataGridViewColumn)TAXES,
			(DataGridViewColumn)EXCISE,
			(DataGridViewColumn)TAXPRC
		});
		((Control)DG).Location = new Point(1, 2);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		((Control)DG).Size = new Size(927, 338);
		((Control)DG).TabIndex = 0;
		((DataGridViewColumn)ID).HeaderText = "ID";
		((DataGridViewColumn)ID).Name = "ID";
		((DataGridViewColumn)ID).ReadOnly = true;
		((DataGridViewColumn)TAXES).HeaderText = "NAME";
		((DataGridViewColumn)TAXES).Name = "TAXES";
		((DataGridViewColumn)TAXES).ReadOnly = true;
		((DataGridViewColumn)TAXES).Width = 250;
		((DataGridViewColumn)EXCISE).HeaderText = "EXCISE";
		((DataGridViewColumn)EXCISE).Name = "EXCISE";
		((DataGridViewColumn)EXCISE).ReadOnly = true;
		((DataGridViewColumn)EXCISE).Width = 250;
		((DataGridViewColumn)TAXPRC).HeaderText = "TAXPRC";
		((DataGridViewColumn)TAXPRC).Name = "TAXPRC";
		((DataGridViewColumn)TAXPRC).ReadOnly = true;
		((DataGridViewColumn)TAXPRC).Width = 200;
		((Control)ADDoperator).Anchor = (AnchorStyles)6;
		((Control)ADDoperator).Location = new Point(753, 357);
		((Control)ADDoperator).Name = "ADDoperator";
		((Control)ADDoperator).Size = new Size(165, 33);
		((Control)ADDoperator).TabIndex = 4;
		((ButtonBase)ADDoperator).Text = "Добавить";
		((ButtonBase)ADDoperator).UseVisualStyleBackColor = true;
		((Control)CashIS).Anchor = (AnchorStyles)6;
		((Control)CashIS).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CashIS).Location = new Point(282, 357);
		((Control)CashIS).Name = "CashIS";
		((Control)CashIS).Size = new Size(260, 24);
		((Control)CashIS).TabIndex = 2;
		CashIS.TextAlign = (HorizontalAlignment)2;
		((Control)PayName).Anchor = (AnchorStyles)6;
		((Control)PayName).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PayName).Location = new Point(10, 357);
		((Control)PayName).Name = "PayName";
		((Control)PayName).Size = new Size(266, 24);
		((Control)PayName).TabIndex = 1;
		PayName.TextAlign = (HorizontalAlignment)2;
		((Control)TAXPRCB).Anchor = (AnchorStyles)6;
		((Control)TAXPRCB).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TAXPRCB).Location = new Point(548, 357);
		((Control)TAXPRCB).Name = "TAXPRCB";
		((Control)TAXPRCB).Size = new Size(199, 24);
		((Control)TAXPRCB).TabIndex = 3;
		TAXPRCB.TextAlign = (HorizontalAlignment)2;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(6f, 13f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(930, 402);
		((Control)this).Controls.Add((Control)(object)TAXPRCB);
		((Control)this).Controls.Add((Control)(object)ADDoperator);
		((Control)this).Controls.Add((Control)(object)CashIS);
		((Control)this).Controls.Add((Control)(object)PayName);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Control)this).Name = "Form4";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "TAXES";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void Form4_Load(object sender, EventArgs e)
	{
		LoadOperators();
	}

	private void LoadOperators()
	{
		checked
		{
			try
			{
				DG.RowCount = 0;
				string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connectionString;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM TAXES";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Form)this).Text = DG.RowCount.ToString();
				while (sQLiteDataReader.Read())
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[0]);
					DG[1, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[1]);
					DG[2, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[2]);
					DG[3, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[3]);
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				((Form)this).Text = "TAXES " + WebCheck.All.l.MaxID("TAXES");
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ADDoperator_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(PayName.Text.Trim(), "", false) == 0)
		{
			((Control)PayName).Focus();
			return;
		}
		if (Operators.CompareString(CashIS.Text.Trim(), "", false) == 0)
		{
			((Control)CashIS).Focus();
			return;
		}
		if (!Versioned.IsNumeric((object)CashIS.Text))
		{
			CashIS.Text = "";
			((Control)CashIS).Focus();
			return;
		}
		if (!Versioned.IsNumeric((object)TAXPRCB.Text))
		{
			TAXPRCB.Text = "";
			((Control)TAXPRCB).Focus();
			return;
		}
		string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = connectionString;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO TAXES (NAME, EXCISE, TAXPRC) VALUES ('" + PayName.Text + "','" + CashIS.Text + "','" + Strings.Replace(TAXPRCB.Text, ",", ".", 1, -1, (CompareMethod)0) + "')";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		PayName.Text = "";
		CashIS.Text = "";
		TAXPRCB.Text = "";
		LoadOperators();
	}
}
