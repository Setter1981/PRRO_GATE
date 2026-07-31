using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormKeys : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("Submit")]
	private Button _Submit;

	internal virtual DataGridView DG
	{
		[CompilerGenerated]
		get
		{
			return _DG;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			DataGridViewCellEventHandler value2 = DG_CellContentClick;
			DataGridViewCellEventHandler value3 = DG_CellClick;
			DataGridViewCellMouseEventHandler value4 = DG_CellMouseUp;
			DataGridViewCellEventHandler value5 = DG_CellEndEdit;
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellContentClick -= value2;
				dG.CellClick -= value3;
				dG.CellMouseUp -= value4;
				dG.CellEndEdit -= value5;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellContentClick += value2;
				dG.CellClick += value3;
				dG.CellMouseUp += value4;
				dG.CellEndEdit += value5;
			}
		}
	}

	internal virtual Button Submit
	{
		[CompilerGenerated]
		get
		{
			return _Submit;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Submit_Click;
			Button submit = _Submit;
			if (submit != null)
			{
				submit.Click -= value2;
			}
			_Submit = value;
			submit = _Submit;
			if (submit != null)
			{
				submit.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("EmailT")]
	internal virtual TextBox EmailT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column1")]
	internal virtual DataGridViewTextBoxColumn Column1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column2")]
	internal virtual DataGridViewTextBoxColumn Column2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column3")]
	internal virtual DataGridViewTextBoxColumn Column3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column4")]
	internal virtual DataGridViewCheckBoxColumn Column4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column5")]
	internal virtual DataGridViewComboBoxColumn Column5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormKeys()
	{
		base.Load += FormKeys_Load;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormKeys));
		this.DG = new System.Windows.Forms.DataGridView();
		this.Submit = new System.Windows.Forms.Button();
		this.EmailT = new System.Windows.Forms.TextBox();
		this.Label2 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		this.Column1 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column2 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column3 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column4 = new System.Windows.Forms.DataGridViewCheckBoxColumn();
		this.Column5 = new System.Windows.Forms.DataGridViewComboBoxColumn();
		((System.ComponentModel.ISupportInitialize)this.DG).BeginInit();
		base.SuspendLayout();
		this.DG.AllowUserToAddRows = false;
		this.DG.AllowUserToDeleteRows = false;
		this.DG.ColumnHeadersHeightSizeMode = System.Windows.Forms.DataGridViewColumnHeadersHeightSizeMode.AutoSize;
		this.DG.Columns.AddRange(this.Column1, this.Column2, this.Column3, this.Column4, this.Column5);
		this.DG.Location = new System.Drawing.Point(5, 77);
		this.DG.Name = "DG";
		this.DG.RowHeadersWidth = 51;
		this.DG.RowTemplate.Height = 24;
		this.DG.Size = new System.Drawing.Size(997, 469);
		this.DG.TabIndex = 0;
		this.Submit.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Submit.Location = new System.Drawing.Point(839, 15);
		this.Submit.Name = "Submit";
		this.Submit.Size = new System.Drawing.Size(137, 34);
		this.Submit.TabIndex = 1;
		this.Submit.Text = "Замовити";
		this.Submit.UseVisualStyleBackColor = true;
		this.EmailT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.EmailT.Location = new System.Drawing.Point(525, 15);
		this.EmailT.Name = "EmailT";
		this.EmailT.Size = new System.Drawing.Size(290, 30);
		this.EmailT.TabIndex = 2;
		this.EmailT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.EmailT.Visible = false;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(446, 18);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(73, 25);
		this.Label2.TabIndex = 3;
		this.Label2.Text = "Email *";
		this.Label2.Visible = false;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(12, 9);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(373, 60);
		this.Label1.TabIndex = 4;
		this.Label1.Text = "Вкажіть на які фіскальні номери Ви хочете\r\nпридбати ліцензію і на вказану адресу, \r\nми відправимо рахунок";
		this.Label1.Visible = false;
		this.Column1.HeaderText = "TIN";
		this.Column1.MinimumWidth = 6;
		this.Column1.Name = "Column1";
		this.Column1.ReadOnly = true;
		this.Column1.Width = 150;
		this.Column2.HeaderText = "FN";
		this.Column2.MinimumWidth = 6;
		this.Column2.Name = "Column2";
		this.Column2.ReadOnly = true;
		this.Column2.Width = 150;
		this.Column3.HeaderText = "Ліцензія";
		this.Column3.MinimumWidth = 6;
		this.Column3.Name = "Column3";
		this.Column3.ReadOnly = true;
		this.Column3.Width = 125;
		this.Column4.HeaderText = "Замовити";
		this.Column4.MinimumWidth = 6;
		this.Column4.Name = "Column4";
		this.Column4.Width = 75;
		this.Column5.HeaderText = "Термін";
		this.Column5.Items.AddRange("12 місяців", "24 місяців", "36 місяців");
		this.Column5.MinimumWidth = 6;
		this.Column5.Name = "Column5";
		this.Column5.ReadOnly = true;
		this.Column5.Width = 125;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1006, 549);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.EmailT);
		base.Controls.Add(this.Submit);
		base.Controls.Add(this.DG);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormKeys";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Ліцензія";
		((System.ComponentModel.ISupportInitialize)this.DG).EndInit();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormKeys_Load(object sender, EventArgs e)
	{
		Application.DoEvents();
		new KeysUpDate().DownLoadKeysAll();
		base.AcceptButton = Submit;
		int num = All.f.IndexMaxFn();
		checked
		{
			for (int i = 1; i <= num; i++)
			{
				string text = All.f.NameFn(i).Trim();
				if (text.Length == 10)
				{
					DG.RowCount++;
					string text2 = All.f.StringGetFn(text, "TIN");
					DG[0, DG.RowCount - 1].Value = text2;
					DG[1, DG.RowCount - 1].Value = text;
					DG[2, DG.RowCount - 1].Value = FullVersionT(text2, text);
				}
			}
		}
	}

	private string FullVersionT(string ttt, string fff)
	{
		KeysWC keysWC = new KeysWC();
		DateTime now = DateTime.Now;
		string text = now.Year.ToString();
		string text2 = now.Month.ToString();
		if (text2.Length < 2)
		{
			text2 = "0" + text2;
		}
		text += text2;
		string text3 = now.Day.ToString();
		if (text3.Length < 2)
		{
			text3 = "0" + text3;
		}
		text += text3;
		long num = keysWC.DhgbK(ttt, fff);
		if (num < Conversions.ToInteger(text))
		{
			return "free";
		}
		string text4 = num.ToString();
		return Conversions.ToString(text4[6]) + Conversions.ToString(text4[7]) + "/" + Conversions.ToString(text4[4]) + Conversions.ToString(text4[5]) + "/" + Conversions.ToString(text4[0]) + Conversions.ToString(text4[1]) + Conversions.ToString(text4[2]) + Conversions.ToString(text4[3]);
	}

	private void DG_CellContentClick(object sender, DataGridViewCellEventArgs e)
	{
	}

	private void DG_CellClick(object sender, DataGridViewCellEventArgs e)
	{
	}

	private void DG_CellMouseUp(object sender, DataGridViewCellMouseEventArgs e)
	{
	}

	private void DG_CellEndEdit(object sender, DataGridViewCellEventArgs e)
	{
		DG[4, DG.CurrentRow.Index].ReadOnly = !Conversions.ToBoolean(DG[3, DG.CurrentRow.Index].Value);
		if (DG[4, DG.CurrentRow.Index].ReadOnly)
		{
			DG[4, DG.CurrentRow.Index].Value = "";
		}
		else
		{
			DG[4, DG.CurrentRow.Index].Value = "12 місяців";
		}
	}

	private void Submit_Click(object sender, EventArgs e)
	{
		OpenURL("https://www.webchek.com.ua/licorder/");
	}

	public void OpenURL(string wwwURL)
	{
		try
		{
			Process.Start(wwwURL);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}
}
